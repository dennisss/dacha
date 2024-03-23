use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::borrow::Borrow;
use std::collections::HashSet;
use std::ops::DerefMut;
use std::sync::Mutex;
use std::sync::RwLock;
use std::{collections::HashMap, sync::Arc};

use common::errors::*;
use executor::sync::{AsyncMutex, AsyncMutexPermit};
use file::{LocalPath, LocalPathBuf};
use protobuf_core::extension::ExtensionTag;
use protobuf_core::tokenizer::serialize_str_lit;
use protobuf_core::MessagePtr;
use protobuf_core::MessageReflection;
use protobuf_core::WireResult;
use protobuf_descriptor::EnumDescriptorProto;
use protobuf_descriptor::EnumValueDescriptorProto;
// use protobuf_builtins::google::protobuf::Any;
use protobuf_core::reflection::Reflect;
use protobuf_core::reflection::ReflectionMut;
use protobuf_core::{FieldDescriptorShort, FieldNumber, Message, StaticMessage};
use protobuf_descriptor as pb;

use crate::spec;
use crate::spec::Syntax;
use crate::DynamicMessage;

/*
Need to access mesage definitions before everything is parsed:
- A can incrementally add all symbols that have no dependencies.
- A 'message' depends on all field types being present + its message options


Step 1: Accumulate all new symbols that we want to add.


*/

#[derive(Clone, Default)]
pub struct DescriptorPoolOptions {
    /// Paths which will be searched when resolving proto file imports.
    ///
    /// (only used in add_local_file())
    ///
    /// - If a .proto file references 'import "y.proto";', then there must be a
    ///   'x' in this list such that 'x/y.proto' exists.
    /// - The first directory in which a match can be found will be used.
    /// - The relative path in one of these paths is also used as the
    ///   FileDescriptorProto::name.
    pub paths: Vec<LocalPathBuf>,
}

impl DescriptorPoolOptions {
    pub fn default_for_workspace(workspace_dir: &LocalPath) -> Self {
        let mut options = Self::default();

        // TODO: Infer these from build dependencies.
        options
            .paths
            .push(workspace_dir.join("third_party/protobuf_builtins/proto"));
        options
            .paths
            .push(workspace_dir.join("third_party/protobuf_descriptor"));
        options
            .paths
            .push(workspace_dir.join("third_party/googleapis/repo"));

        options.paths.push(workspace_dir.to_owned());

        options
    }
}

#[derive(Clone)]
pub struct DescriptorPool {
    shared: Arc<DescriptorPoolShared>,
}

struct DescriptorPoolShared {
    options: DescriptorPoolOptions,

    /// Lock that must be help if preparing to mutate 'state'.
    writer_lock: AsyncMutex<()>,

    state: RwLock<DescriptorPoolState>,
}

struct DescriptorPoolState {
    /// Map from file name (relative path in root directory) to the descriptor
    files: HashMap<String, Arc<FileDescriptorInner>>,

    /// Map from the fully qualified name of each symbol in this pool to it's
    /// descriptor object.
    types: HashMap<TypeName, TypeDescriptorInner>,
}

/// An in-progress change to a DescriptorPool.
///
/// - At most one of these will ever exist in a single pool.
/// - This struct queues up all the types/symbols that will be added before
///   adding them based on dependency rules in finish_write().
struct PendingWrite<'a> {
    guard: AsyncMutexPermit<'a, ()>,
    next_file_index: u32,
    new_types: Vec<(TypeName, TypeDescriptorInner)>,
    new_files: Vec<Arc<FileDescriptorInner>>,
}

struct RegistrationScope {
    file_index: u32,
    syntax: Syntax,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct TypeName {
    name: Arc<String>,
}

impl TypeName {
    fn new(s: String) -> Self {
        Self { name: Arc::new(s) }
    }
}

impl Borrow<str> for TypeName {
    fn borrow(&self) -> &str {
        self.name.as_str()
    }
}

impl DescriptorPool {
    /// Creates a new empty descriptor pool.
    pub fn new(options: DescriptorPoolOptions) -> Self {
        Self {
            shared: Arc::new(DescriptorPoolShared {
                options,
                writer_lock: AsyncMutex::new(()),
                state: RwLock::new(DescriptorPoolState {
                    files: HashMap::new(),
                    types: HashMap::new(),
                }),
            }),
        }
    }

    pub fn find_file(&self, name: &str) -> Option<FileDescriptor> {
        let state = self.shared.state.read().unwrap();
        state.files.get(name).map(|desc| FileDescriptor {
            pool: self.clone(),
            inner: desc.clone(),
        })
    }

    /// Parses a .proto file located in the filesystem and adds it to the pool.
    ///
    /// - If the file already exists in the pool, then it won't be re-added.
    /// - Any imported dependencies will also be added to the pool.
    ///
    /// NOTE: It is undefined behavior to add descriptors to the pool which have
    /// different root directories.
    pub async fn add_file<P: AsRef<file::LocalPath>>(&self, path: P) -> Result<FileDescriptor> {
        let mut write = self.begin_write().await;

        let mut root_path = path.as_ref().normalized();

        let mut pending_paths: Vec<LocalPathBuf> = vec![];
        let mut visited_paths = HashSet::new();

        pending_paths.push(root_path.clone());
        visited_paths.insert(root_path.as_str().to_string());

        let mut proto_nodes = HashMap::new();

        while let Some(path) = pending_paths.pop() {
            // TODO: Is it possible for two files at two different paths to have the same
            // name?
            let name = self.resolve_file_name(&path)?;

            if self.shared.state.read().unwrap().files.contains_key(&name) {
                // TODO: Check that it also has the same file path?
                continue;
            }

            let proto_file_src = file::read_to_string(&path).await?;
            let mut proto_file = crate::syntax::parse_proto(&proto_file_src)?.to_proto();

            proto_file.set_name(&name);

            for dep in proto_file.dependency_mut() {
                let relative_path = LocalPath::new(dep.as_str());

                // TODO: Verify that this works.
                if relative_path != &relative_path.normalized() {
                    // We don't allow import paths of the form "../file.proto".
                    return Err(err_msg("Non-relative important path"));
                }

                if relative_path.extension().unwrap_or_default() != "proto" {
                    return Err(err_msg(
                        "Expected a .proto extension for imported proto files",
                    ));
                }

                let mut full_path = None;
                for base_path in &self.shared.options.paths {
                    let p: LocalPathBuf = base_path.join(&relative_path).normalized();
                    if !file::exists(&p).await? {
                        continue;
                    }

                    full_path = Some(p);
                    break;
                }

                let full_path = full_path
                    .ok_or_else(|| format_err!("Imported proto file not found: {}", dep))?;

                // Normalize the name based on the import path ordering that we are using.
                // TODO: Return an error if this changes the path?
                *dep = self.resolve_file_name(&full_path)?;

                // TODO: Also detect cycles.
                if !visited_paths.insert(full_path.as_str().to_string()) {
                    continue;
                }

                pending_paths.push(full_path);
            }

            proto_nodes.insert(name.clone(), (proto_file, path));
        }

        let file_ordering = Self::topological_sort(&proto_nodes)?;
        assert_eq!(file_ordering.len(), proto_nodes.len());

        // Register types starting with dependencies first so that finish_write() is
        // more likely to succeed.
        for key in file_ordering {
            let (proto_file, path) = proto_nodes.remove(&key).unwrap();

            self.register_file_descriptor_proto(proto_file, Some(path), &mut write)?;
        }

        self.finish_write(write)?;

        // // Step 2: Acquire lock and start adding things into the state.
        // {
        //     let mut state = self.shared.state.write().unwrap();

        //     for (proto_file, path) in parsed_protos {
        //         self.add_file_descriptor_proto(proto_file, Some(path.clone()), &mut
        // state)?;     }

        //     self.process_new_types(&mut state)?;
        // }

        let root_desc = self
            .shared
            .state
            .read()
            .unwrap()
            .files
            .get(&self.resolve_file_name(&root_path)?)
            .unwrap()
            .clone();

        Ok(FileDescriptor {
            pool: self.clone(),
            inner: root_desc,
        })
    }

    // TODO: Deduplicate this code around the code base.
    //
    // TODO: The one of the 'compute' crate has a bug in it.
    fn topological_sort(
        nodes: &HashMap<String, (pb::FileDescriptorProto, LocalPathBuf)>,
    ) -> Result<Vec<String>> {
        let mut ordering = vec![];

        if nodes.is_empty() {
            // Could happen if all files were already added to the pool.
            return Ok(ordering);
        }

        fn visit(
            node_id: String,
            nodes: &HashMap<String, (pb::FileDescriptorProto, LocalPathBuf)>,
            pending: &mut HashMap<String, bool>,
            ordering: &mut Vec<String>,
        ) -> Result<()> {
            if !pending.contains_key(&node_id) {
                return Ok(());
            }

            if *pending.get(&node_id).unwrap() {
                // Cyclic loop in the graph.
                return Err(err_msg("Cycle in import dependencies."));
            }

            pending.insert(node_id.clone(), true);

            let node = &nodes.get(&node_id).unwrap().0;
            for dep in node.dependency() {
                if !nodes.contains_key(dep) {
                    continue;
                }

                visit(dep.clone(), nodes, pending, ordering)?;
            }

            pending.remove(&node_id);
            ordering.push(node_id);

            Ok(())
        };

        // Value is whether or not it has a 'temporary marking'
        let mut pending = HashMap::new();
        for key in nodes.keys() {
            pending.insert(key.clone(), false);
        }

        while let Some(node_id) = pending.keys().next().cloned() {
            visit(node_id, nodes, &mut pending, &mut ordering)?;
        }

        Ok(ordering)
    }

    /// Starts a new write/mutation to the descriptor pool.
    async fn begin_write(&self) -> PendingWrite {
        let guard = self.shared.writer_lock.lock().await.unwrap();

        let next_file_index = self.shared.state.read().unwrap().files.len() as u32;

        PendingWrite {
            guard,
            next_file_index,
            new_types: vec![],
            new_files: vec![],
        }
    }

    /// Finds the canonical name of a .proto file which is derived from its
    /// relative positive to the pool's search paths.
    fn resolve_file_name(&self, path: &LocalPath) -> Result<String> {
        let mut relative_path = None;
        for base_path in &self.shared.options.paths {
            if let Some(p) = path.strip_prefix(base_path) {
                relative_path = Some(p);
                break;
            }
        }

        let relative_path = relative_path
            .ok_or_else(|| format_err!("Path is not in the protobuf paths: {:?}", path))?;

        Ok(relative_path.to_string())
    }

    /// Adds a single binary serialized FileDescriptorProto representing a
    /// single .proto file to the pool.
    pub async fn add_file_descriptor(&self, data: &[u8]) -> Result<()> {
        let mut write = self.begin_write().await;

        let proto = pb::FileDescriptorProto::parse(data)?;

        // Don't re-add files
        if self
            .shared
            .state
            .read()
            .unwrap()
            .files
            .contains_key(proto.name())
        {
            return Ok(());
        }

        self.register_file_descriptor_proto(proto, None, &mut write)?;

        self.finish_write(write)?;

        Ok(())
    }

    // Assumes you already have the writer lock.
    // The caller is responsible for ensuring that no duplicate files are added.
    fn register_file_descriptor_proto(
        &self,
        mut proto: pb::FileDescriptorProto,
        local_path: Option<LocalPathBuf>,
        write: &mut PendingWrite,
    ) -> Result<()> {
        let syntax = match proto.syntax() {
            "proto2" => Syntax::Proto2,
            "proto3" => Syntax::Proto3,
            _ => {
                return Err(err_msg("Unsupported proto syntax."));
            }
        };

        let mut children = vec![];
        let file_index = {
            let i = write.next_file_index;
            write.next_file_index += 1;
            i
        };

        let scope = RegistrationScope { file_index, syntax };

        let path = TypeName::new(proto.package().to_string());

        // TODO: Drain the types instead of cloning them.
        for m in proto.message_type() {
            children.push(self.register_message_descriptor(
                m.as_ref().clone(),
                &path,
                &scope,
                write,
            )?);
        }
        proto.message_type_mut().clear();

        for e in proto.enum_type() {
            children.push(self.register_enum_descriptor(
                e.as_ref().clone(),
                &path,
                &scope,
                write,
            )?);
        }
        proto.enum_type_mut().clear();

        for s in proto.service() {
            children.push(self.register_service_descriptor(
                s.as_ref().clone(),
                &path,
                &scope,
                write,
            )?);
        }
        proto.service_mut().clear();

        for e in proto.extension() {
            children.push(self.register_extension_descriptor(
                e.as_ref().clone(),
                &path,
                &scope,
                write,
            )?);
        }
        proto.extension_mut().clear();

        let name = proto.name().to_string();
        let desc = Arc::new(FileDescriptorInner {
            name: name.clone(),
            index: file_index,
            local_path,
            proto,
            syntax,
            children,
        });

        write.new_files.push(desc);

        Ok(())
    }

    /// Adds a MessageDescriptor to an ongoing write to the pool.
    fn register_message_descriptor(
        &self,
        mut proto: pb::DescriptorProto,
        path: &TypeName,
        scope: &RegistrationScope,
        write: &mut PendingWrite,
    ) -> Result<TypeName> {
        let mut name = self.concat_names(path, proto.name());

        let mut children = vec![];

        for m in proto.nested_type() {
            children.push(self.register_message_descriptor(
                m.as_ref().clone(),
                &name,
                scope,
                write,
            )?);
        }
        proto.nested_type_mut().clear();

        for e in proto.enum_type() {
            children.push(self.register_enum_descriptor(
                e.as_ref().clone(),
                &name,
                scope,
                write,
            )?);
        }
        proto.enum_type_mut().clear();

        for e in proto.extension() {
            children.push(self.register_extension_descriptor(
                e.as_ref().clone(),
                &name,
                scope,
                write,
            )?);
        }
        proto.extension_mut().clear();

        let type_url = format!("{}{}", protobuf_core::TYPE_URL_PREFIX, name.name.as_str());

        let mut fields_short = vec![];
        for field in proto.field() {
            fields_short.push(FieldDescriptorShort::new(
                field.name().to_string(),
                field.number() as u32,
            ));
        }

        let desc = TypeDescriptorInner::Message(Arc::new(MessageDescriptorInner {
            name: name.clone(),
            file_index: scope.file_index,
            type_url,
            syntax: scope.syntax,
            proto,
            fields_short,
            children,
        }));
        write.new_types.push((name.clone(), desc));

        Ok(name)
    }

    /// Adds a EnumDescriptor to an ongoing write to the pool.
    fn register_enum_descriptor(
        &self,
        proto: pb::EnumDescriptorProto,
        path: &TypeName,
        scope: &RegistrationScope,
        write: &mut PendingWrite,
    ) -> Result<TypeName> {
        let name = self.concat_names(path, proto.name());

        let desc = TypeDescriptorInner::Enum(Arc::new(EnumDescriptorInner {
            name: name.clone(),
            file_index: scope.file_index,
            proto,
        }));
        write.new_types.push((name.clone(), desc));

        Ok(name)
    }

    /// Adds a ServiceDescriptor to an ongoing write to the pool.
    fn register_service_descriptor(
        &self,
        proto: pb::ServiceDescriptorProto,
        path: &TypeName,
        scope: &RegistrationScope,
        write: &mut PendingWrite,
    ) -> Result<TypeName> {
        let name = self.concat_names(path, proto.name());

        let desc = TypeDescriptorInner::Service(Arc::new(ServiceDescriptorInner {
            name: name.clone(),
            file_index: scope.file_index,
            proto,
        }));
        write.new_types.push((name.clone(), desc));

        Ok(name)
    }

    /// Adds a ExtensionDescriptor to an ongoing write to the pool.
    fn register_extension_descriptor(
        &self,
        proto: pb::FieldDescriptorProto,
        path: &TypeName,
        scope: &RegistrationScope,
        write: &mut PendingWrite,
    ) -> Result<TypeName> {
        let name = self.concat_names(path, proto.name());

        let desc = TypeDescriptorInner::Extension(Arc::new(ExtendDescriptorInner {
            name: name.clone(),
            file_index: scope.file_index,
            proto,
        }));
        write.new_types.push((name.clone(), desc));

        Ok(name)
    }

    /// Finalizes a mutation by adding all the new symbols to the pool.
    ///
    /// During this stage we also interpret all the uninterpreted_options in the
    /// descriptors. Because extensions must be registered in the pool (in
    /// addition to the extension's options being fully interpreted) before an
    /// option referencing that extension can be interpreted, this function will
    /// register the types in a dependency graph ordering. Additionally due to
    /// the fact that extensions may contain DynamicMessages that need read
    /// access to the pool to be constructed, we must construct options using a
    /// reader lock late and only later add the types to the pool using a writer
    /// lock.
    ///
    /// The current algorithm requires many passes over all types if there are
    /// many complex extension relationships, but should normally finish in one
    /// pass.
    fn finish_write(&self, mut write: PendingWrite) -> Result<()> {
        // TODO: Perform a topological sort of the types (Note that we only need to sort
        // types within a single file and we don't need to do this if we were given a
        // pre-generated FileDescriptorProto).

        for (name, mut desc) in write.new_types {
            let state = self.shared.state.read().unwrap();
            self.resolve_type_options(&state, &mut desc)?;
            drop(state);

            let mut state = self.shared.state.write().unwrap();
            if state.types.insert(name.clone(), desc).is_some() {
                return Err(format_err!("Duplicate type named {}", name.name));
            }
        }

        for mut desc in write.new_files {
            let state = self.shared.state.read().unwrap();
            self.resolve_file_options(&state, &mut desc);
            drop(state);

            let mut state = self.shared.state.write().unwrap();
            if state
                .files
                .insert(desc.name.clone(), desc.clone())
                .is_some()
            {
                return Err(format_err!("Duplicate file named {}", desc.name));
            }
        }

        Ok(())
    }

    fn resolve_file_options(
        &self,
        state: &DescriptorPoolState,
        desc: &mut Arc<FileDescriptorInner>,
    ) -> Result<()> {
        let v = Arc::get_mut(desc).unwrap();

        let uninterpreted_options = v
            .proto
            .options_mut()
            .uninterpreted_option_mut()
            .split_off(0);

        let scope = v.proto.package().to_string();
        self.apply_uninterpreted_options(
            &uninterpreted_options,
            &scope,
            state,
            v.proto.options_mut(),
        )?;

        Ok(())
    }

    fn resolve_type_options(
        &self,
        state: &DescriptorPoolState,
        desc: &mut TypeDescriptorInner,
    ) -> Result<()> {
        match desc {
            TypeDescriptorInner::Message(v) => {
                let mut v = Arc::get_mut(v).unwrap();
                let scope = &v.name.name;

                let uninterpreted_options = v
                    .proto
                    .options_mut()
                    .uninterpreted_option_mut()
                    .split_off(0);

                self.apply_uninterpreted_options(
                    &uninterpreted_options,
                    scope,
                    state,
                    v.proto.options_mut(),
                )?;

                for field in v.proto.field_mut() {
                    self.resolve_field_options(scope, state, field.as_mut())?;
                }
            }
            // TODO: Don't forget to recurse for these.
            TypeDescriptorInner::Enum(v) => {
                let mut v = Arc::get_mut(v).unwrap();
                let scope = &v.name.name;

                let uninterpreted_options = v
                    .proto
                    .options_mut()
                    .uninterpreted_option_mut()
                    .split_off(0);

                self.apply_uninterpreted_options(
                    &uninterpreted_options,
                    scope,
                    state,
                    v.proto.options_mut(),
                )?;

                for v in v.proto.value_mut() {
                    let uninterpreted_options =
                        v.options_mut().uninterpreted_option_mut().split_off(0);

                    self.apply_uninterpreted_options(
                        &uninterpreted_options,
                        scope,
                        state,
                        v.options_mut(),
                    )?;
                }
            }
            TypeDescriptorInner::Service(v) => {
                let mut v = Arc::get_mut(v).unwrap();
                let scope = &v.name.name;

                let uninterpreted_options = v
                    .proto
                    .options_mut()
                    .uninterpreted_option_mut()
                    .split_off(0);

                self.apply_uninterpreted_options(
                    &uninterpreted_options,
                    scope,
                    state,
                    v.proto.options_mut(),
                )?;

                for v in v.proto.method_mut() {
                    let uninterpreted_options =
                        v.options_mut().uninterpreted_option_mut().split_off(0);

                    self.apply_uninterpreted_options(
                        &uninterpreted_options,
                        scope,
                        state,
                        v.options_mut(),
                    )?;
                }
            }
            TypeDescriptorInner::Extension(v) => {
                let mut v = Arc::get_mut(v).unwrap();
                let scope = &v.name.name;

                self.resolve_field_options(scope, state, &mut v.proto)?;
            }
        }

        Ok(())
    }

    fn resolve_field_options(
        &self,
        scope: &str,
        state: &DescriptorPoolState,
        proto: &mut pb::FieldDescriptorProto,
    ) -> Result<()> {
        //
        let mut uninterpreted_options = proto.options_mut().uninterpreted_option_mut().split_off(0);

        for i in 0..uninterpreted_options.len() {
            let opt = &uninterpreted_options[i];
            let is_default = opt.name_len() == 1
                && opt.name()[0].name_part() == "default"
                && !opt.name()[0].is_extension();
            if !is_default {
                continue;
            }

            // TODO: Check field type compatibility.
            if opt.has_positive_int_value() {
                proto.set_default_value(opt.positive_int_value().to_string());
            } else if opt.has_negative_int_value() {
                proto.set_default_value(opt.negative_int_value().to_string());
            } else if opt.has_double_value() {
                proto.set_default_value(opt.double_value().to_string());
            } else if opt.has_identifier_value() {
                proto.set_default_value(opt.identifier_value());
            } else if opt.has_string_value() {
                // TODO: Only serialize if the field type is bytes?
                serialize_str_lit(opt.string_value(), proto.default_value_mut());
            } else if opt.has_aggregate_value() {
                proto.set_default_value(opt.aggregate_value());
            } else {
                return Err(err_msg("Option has no value"));
            }

            uninterpreted_options.remove(i);
            break;
        }

        self.apply_uninterpreted_options(
            &uninterpreted_options,
            scope,
            state,
            proto.options_mut(),
        )?;

        Ok(())
    }

    fn apply_uninterpreted_options(
        &self,
        uninterpreted_options: &[MessagePtr<pb::UninterpretedOption>],
        scope: &str,
        state: &DescriptorPoolState,
        options: &mut dyn MessageReflection,
    ) -> Result<()> {
        // TODO: Prevent adding things to the 'uninterpreted_options' field.

        for opt in uninterpreted_options {
            // Resolve the inner field which the option is referring to.
            let mut target = Some(ReflectionMut::Message(options));
            for name_part in opt.name() {
                let target_message = match target.take().unwrap() {
                    ReflectionMut::Message(m) => m,
                    _ => return Err(err_msg("Attempting to take a field of a non-message")),
                };

                if name_part.is_extension() {
                    let extension_desc = self
                        .find_relative_type_inner(scope, name_part.name_part(), state)
                        .ok_or_else(|| format_err!("Failed to find extension type: {:?}", opt))?
                        .to_extension()
                        .ok_or_else(|| err_msg("Not an extension"))?;

                    // TODO: Check that the 'extendee' is correct.

                    let extension_set = target_message
                        .extensions_mut()
                        .ok_or_else(|| err_msg("Target doesn't support extensions"))?;

                    target = Some(
                        extension_set
                            .get_dynamic_mut(&extension_desc)?
                            .reflect_mut(),
                    );
                } else {
                    let num = target_message
                        .field_number_by_name(name_part.name_part())
                        .ok_or_else(|| {
                            format_err!(
                                "Unknown field in option name part: {} in {:?}",
                                name_part.name_part(),
                                opt
                            )
                        })?;

                    target = Some(target_message.field_by_number_mut(num).unwrap());
                }
            }

            let target = target.unwrap();
            Self::apply_option_value_to_reflection(opt.as_ref(), target)?;
        }

        Ok(())
    }

    fn apply_option_value_to_reflection(
        opt: &pb::UninterpretedOption,
        target: ReflectionMut,
    ) -> Result<()> {
        // TODO: Also check that values are in range (don't overflow the int
        // max value).
        match target {
            ReflectionMut::F32(v) => {
                if opt.has_double_value() {
                    *v = opt.double_value() as f32;
                } else if opt.has_positive_int_value() {
                    *v = opt.positive_int_value() as f32;
                } else if opt.has_negative_int_value() {
                    *v = opt.negative_int_value() as f32;
                } else {
                    return Err(err_msg("Incompatible option type"));
                }
            }
            ReflectionMut::F64(v) => {
                if opt.has_double_value() {
                    *v = opt.double_value() as f64;
                } else if opt.has_positive_int_value() {
                    *v = opt.positive_int_value() as f64;
                } else if opt.has_negative_int_value() {
                    *v = opt.negative_int_value() as f64;
                } else {
                    return Err(err_msg("Incompatible option type"));
                }
            }
            ReflectionMut::I32(v) => {
                if opt.has_positive_int_value() {
                    *v = opt.positive_int_value() as i32;
                } else if opt.has_negative_int_value() {
                    *v = opt.negative_int_value() as i32;
                } else {
                    return Err(err_msg("Incompatible option type"));
                }
            }
            ReflectionMut::I64(v) => {
                if opt.has_positive_int_value() {
                    *v = opt.positive_int_value() as i64;
                } else if opt.has_negative_int_value() {
                    *v = opt.negative_int_value() as i64;
                } else {
                    return Err(err_msg("Incompatible option type"));
                }
            }
            ReflectionMut::U32(v) => {
                if opt.has_positive_int_value() {
                    *v = opt.positive_int_value() as u32;
                } else {
                    return Err(err_msg("Incompatible option type"));
                }
            }
            ReflectionMut::U64(v) => {
                if opt.has_positive_int_value() {
                    *v = opt.positive_int_value() as u64;
                } else {
                    return Err(err_msg("Incompatible option type"));
                }
            }
            ReflectionMut::Bool(v) => {
                if opt.identifier_value() == "true" {
                    *v = true;
                } else if opt.identifier_value() == "false" {
                    *v = false;
                } else {
                    return Err(err_msg("Incompatible option type"));
                }
            }
            ReflectionMut::String(v) => {
                if !opt.has_string_value() {
                    return Err(err_msg("Incompatible option type"));
                }

                let s = std::str::from_utf8(opt.string_value())?;
                v.clear();
                v.push_str(s);
            }
            ReflectionMut::Bytes(v) => {
                if !opt.has_string_value() {
                    return Err(err_msg("Incompatible option type"));
                }

                v.extend_from_slice(opt.string_value());
            }
            ReflectionMut::Repeated(v) => {
                Self::apply_option_value_to_reflection(opt, v.reflect_add())?;
            }
            ReflectionMut::Message(v) => {
                if !opt.has_aggregate_value() {
                    return Err(err_msg("Incompatible option type"));
                }

                protobuf_core::text::parse_text_proto(opt.aggregate_value(), v)?;
            }
            ReflectionMut::Enum(v) => {
                if opt.has_identifier_value() {
                    v.assign_name(opt.identifier_value())?;
                } else {
                    return Err(err_msg("Incompatible option type"));
                }
            }
            ReflectionMut::Set(_) => todo!(),
        }

        Ok(())
    }

    fn concat_names(&self, base: &TypeName, name: &str) -> TypeName {
        if base.name.is_empty() {
            TypeName::new(name.to_string())
        } else {
            TypeName::new(format!("{}.{}", base.name, name))
        }
    }

    /*
    fn insert_unique_symbol(
        &self,
        name: &TypeName,
        value: TypeDescriptorInner,
        state: &mut DescriptorPoolState,
    ) -> Result<()> {
        if state.types.insert(name.clone(), value).is_some() {
            return Err(format_err!("Duplicate type named {}", name.name));
        }

        state.new_types.push(name.clone());

        Ok(())
    }
    */

    pub fn find_relative_type(&self, scope: &str, relative_name: &str) -> Option<TypeDescriptor> {
        let state = self.shared.state.read().unwrap();
        self.find_relative_type_inner(scope, relative_name, &state)
    }

    fn find_relative_type_inner(
        &self,
        scope: &str,
        relative_name: &str,
        state: &DescriptorPoolState,
    ) -> Option<TypeDescriptor> {
        // TODO: This needs to check for visibility (assuming we are currently in a
        // file, we shouldn't be able to access types unless they are directly
        // imported).

        // Handle the absolute case
        if let Some(absolute_name) = relative_name.strip_prefix('.') {
            return state
                .types
                .get(absolute_name)
                .map(|desc| TypeDescriptor::new(self.clone(), desc.clone()));
        }

        let mut scope_parts = scope.split('.').collect::<Vec<_>>();
        if scope.is_empty() {
            scope_parts.pop();
        }

        let mut current_prefix = &scope_parts[..];
        loop {
            let name = self.concat_names(&TypeName::new(current_prefix.join(".")), relative_name);

            if let Some(desc) = state.types.get(&name) {
                return Some(TypeDescriptor::new(self.clone(), desc.clone()));
            }

            if current_prefix.len() > 0 {
                // For path 'x.y.z', try 'x.y' next time.
                current_prefix = &current_prefix[0..(current_prefix.len() - 1)];
            } else {
                break;
            }
        }

        None
    }
}

impl protobuf_core::message_factory::MessageFactory for DescriptorPool {
    fn new_message(&self, type_url: &str) -> Option<Box<dyn MessageReflection>> {
        let path = match type_url.strip_prefix(protobuf_core::TYPE_URL_PREFIX) {
            Some(v) => v,
            None => return None,
        };

        let desc = match self
            .find_relative_type("", path)
            .and_then(|d| d.to_message())
        {
            Some(v) => v,
            None => return None,
        };

        Some(Box::new(crate::message::DynamicMessage::new(desc)))
    }
}

#[derive(Clone)]
pub struct FileDescriptor {
    pool: DescriptorPool,
    inner: Arc<FileDescriptorInner>,
}

struct FileDescriptorInner {
    name: String,
    index: u32,

    /// Path to the .proto file if this descriptor was loaded from a local file.
    local_path: Option<LocalPathBuf>,

    proto: pb::FileDescriptorProto,

    syntax: Syntax,
    children: Vec<TypeName>,
}

impl FileDescriptor {
    pub fn name(&self) -> &str {
        &self.inner.name
    }

    pub fn syntax(&self) -> Syntax {
        self.inner.syntax
    }

    /// NOTE: This will be sparse.
    pub fn proto(&self) -> &pb::FileDescriptorProto {
        &self.inner.proto
    }

    pub fn to_proto(&self) -> pb::FileDescriptorProto {
        let mut proto = self.inner.proto.clone();
        proto.set_name(self.name());

        let state = self.pool.shared.state.read().unwrap();
        for child_name in &self.inner.children {
            let child_desc = state.types.get(child_name).unwrap();

            match child_desc {
                TypeDescriptorInner::Message(v) => {
                    proto.add_message_type(v.to_proto(&state));
                }
                TypeDescriptorInner::Enum(v) => {
                    proto.add_enum_type(v.to_proto());
                }
                TypeDescriptorInner::Service(v) => {
                    proto.add_service(v.to_proto());
                }
                TypeDescriptorInner::Extension(v) => {
                    proto.add_extension(v.to_proto());
                }
            }
        }

        proto
    }

    pub fn pool(&self) -> &DescriptorPool {
        &self.pool
    }

    pub fn index(&self) -> u32 {
        self.inner.index
    }

    pub fn local_path(&self) -> Option<&LocalPath> {
        self.inner.local_path.as_ref().map(|p| p.as_path())
    }

    pub fn top_level_defs<'a>(&'a self) -> impl Iterator<Item = TypeDescriptor> + 'a {
        let state = self.pool.shared.state.read().unwrap();
        let pool = self.pool.clone();
        self.inner
            .children
            .iter()
            .map(move |t| TypeDescriptor::new(pool.clone(), state.types.get(t).unwrap().clone()))
    }
}

pub enum TypeDescriptor {
    Message(MessageDescriptor),
    Enum(EnumDescriptor),
    Service(ServiceDescriptor),
    Extend(ExtendDescriptor),
}

#[derive(Clone)]
enum TypeDescriptorInner {
    Message(Arc<MessageDescriptorInner>),
    Enum(Arc<EnumDescriptorInner>),
    Service(Arc<ServiceDescriptorInner>),
    Extension(Arc<ExtendDescriptorInner>),
}

impl TypeDescriptor {
    fn new(pool: DescriptorPool, inner: TypeDescriptorInner) -> Self {
        match inner {
            TypeDescriptorInner::Message(m) => {
                TypeDescriptor::Message(MessageDescriptor { pool, inner: m })
            }
            TypeDescriptorInner::Enum(e) => TypeDescriptor::Enum(EnumDescriptor { pool, inner: e }),
            TypeDescriptorInner::Service(s) => {
                TypeDescriptor::Service(ServiceDescriptor { pool, inner: s })
            }
            TypeDescriptorInner::Extension(e) => {
                TypeDescriptor::Extend(ExtendDescriptor { pool, inner: e })
            }
        }
    }

    pub fn to_message(self) -> Option<MessageDescriptor> {
        match self {
            TypeDescriptor::Message(v) => Some(v),
            _ => None,
        }
    }

    pub fn to_enum(self) -> Option<EnumDescriptor> {
        match self {
            TypeDescriptor::Enum(v) => Some(v),
            _ => None,
        }
    }

    pub fn to_service(self) -> Option<ServiceDescriptor> {
        match self {
            TypeDescriptor::Service(v) => Some(v),
            _ => None,
        }
    }

    pub fn to_extension(self) -> Option<ExtendDescriptor> {
        match self {
            TypeDescriptor::Extend(v) => Some(v),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct MessageDescriptor {
    pool: DescriptorPool,
    inner: Arc<MessageDescriptorInner>,
}

impl PartialEq for MessageDescriptor {
    fn eq(&self, other: &Self) -> bool {
        self.inner.type_url == other.inner.type_url
    }
}

impl MessageDescriptor {
    pub fn type_url(&self) -> &str {
        &self.inner.type_url
    }

    pub fn syntax(&self) -> Syntax {
        self.inner.syntax
    }

    pub fn name(&self) -> &str {
        &self.inner.name.name
    }

    pub fn proto(&self) -> &pb::DescriptorProto {
        &self.inner.proto
    }

    pub fn file_index(&self) -> u32 {
        self.inner.file_index
    }

    pub fn nested_messages<'a>(&'a self) -> impl Iterator<Item = MessageDescriptor> + 'a {
        let pool = self.pool.clone();

        self.inner.children.iter().filter_map(move |child_name| {
            let state = pool.shared.state.read().unwrap();
            let desc = state.types.get(child_name).unwrap();
            match desc {
                TypeDescriptorInner::Message(v) => Some(MessageDescriptor {
                    pool: pool.clone(),
                    inner: v.clone(),
                }),
                _ => None,
            }
        })
    }

    pub fn nested_enums<'a>(&'a self) -> impl Iterator<Item = EnumDescriptor> + 'a {
        let pool = self.pool.clone();

        self.inner.children.iter().filter_map(move |child_name| {
            let state = pool.shared.state.read().unwrap();
            let desc = state.types.get(child_name).unwrap();
            match desc {
                TypeDescriptorInner::Enum(v) => Some(EnumDescriptor {
                    pool: pool.clone(),
                    inner: v.clone(),
                }),
                _ => None,
            }
        })
    }

    pub fn nested_extensions<'a>(&'a self) -> impl Iterator<Item = ExtendDescriptor> + 'a {
        let pool = self.pool.clone();

        self.inner.children.iter().filter_map(move |child_name| {
            let state = pool.shared.state.read().unwrap();
            let desc = state.types.get(child_name).unwrap();
            match desc {
                TypeDescriptorInner::Extension(v) => Some(ExtendDescriptor {
                    pool: pool.clone(),
                    inner: v.clone(),
                }),
                _ => None,
            }
        })
    }

    pub fn fields(&self) -> impl Iterator<Item = FieldDescriptor> {
        let msg = self.clone();

        (0..self.inner.proto.field_len()).map(move |i| FieldDescriptor {
            message: msg.clone(),
            field_index: i,
        })
    }

    pub fn oneofs<'a>(&'a self) -> impl Iterator<Item = OneOfDescriptor> + 'a {
        (0..self.proto().oneof_decl_len()).map(move |index| OneOfDescriptor {
            message: self.clone(),
            index,
        })
    }

    pub fn fields_short(&self) -> &[FieldDescriptorShort] {
        &self.inner.fields_short
    }

    pub fn field_by_number(&self, num: FieldNumber) -> Option<FieldDescriptor> {
        for i in 0..self.inner.proto.field_len() {
            let field: &protobuf_descriptor::FieldDescriptorProto = &self.inner.proto.field()[i];
            if field.number() == num as i32 {
                return Some(FieldDescriptor {
                    message: self.clone(),
                    field_index: i,
                });
            }
        }

        None
    }

    pub fn field_number_by_name(&self, name: &str) -> Option<FieldNumber> {
        for i in 0..self.inner.proto.field_len() {
            let field: &protobuf_descriptor::FieldDescriptorProto = &self.inner.proto.field()[i];
            if field.name() == name {
                return Some(field.number() as FieldNumber);
            }
        }

        None
    }
}

struct MessageDescriptorInner {
    name: TypeName,
    file_index: u32,

    type_url: String,
    syntax: Syntax,
    proto: pb::DescriptorProto,
    fields_short: Vec<FieldDescriptorShort>,

    children: Vec<TypeName>,
}

impl MessageDescriptorInner {
    fn to_proto(&self, state: &DescriptorPoolState) -> pb::DescriptorProto {
        let mut proto = self.proto.clone();

        for child_name in &self.children {
            let child_desc = state.types.get(child_name).unwrap();

            match child_desc {
                TypeDescriptorInner::Message(v) => {
                    proto.add_nested_type(v.to_proto(state));
                }
                TypeDescriptorInner::Enum(v) => {
                    proto.add_enum_type(v.to_proto());
                }
                TypeDescriptorInner::Extension(v) => {
                    proto.add_extension(v.to_proto());
                }
                // Should never appear in a message
                TypeDescriptorInner::Service(_) => todo!(),
            }
        }

        proto
    }
}

#[derive(Clone)]
pub struct ServiceDescriptor {
    pool: DescriptorPool,
    inner: Arc<ServiceDescriptorInner>,
}

struct ServiceDescriptorInner {
    name: TypeName,
    file_index: u32,
    proto: pb::ServiceDescriptorProto,
}

impl ServiceDescriptor {
    pub fn proto(&self) -> &protobuf_descriptor::ServiceDescriptorProto {
        &self.inner.proto
    }

    pub fn name(&self) -> &str {
        &self.inner.name.name
    }

    pub fn methods(&self) -> impl Iterator<Item = MethodDescriptor> {
        (0..self.method_len()).map(move |i| self.method(i).unwrap())
    }

    pub fn method(&self, index: usize) -> Option<MethodDescriptor> {
        if index >= self.proto().method_len() {
            return None;
        }

        Some(MethodDescriptor {
            service: self,
            method: &self.proto().method()[index],
        })
    }

    pub fn method_len(&self) -> usize {
        self.proto().method_len()
    }
}

impl ServiceDescriptorInner {
    fn to_proto(&self) -> pb::ServiceDescriptorProto {
        self.proto.clone()
    }
}

pub struct MethodDescriptor<'a> {
    service: &'a ServiceDescriptor,
    method: &'a pb::MethodDescriptorProto,
}

impl<'a> MethodDescriptor<'a> {
    pub fn proto(&self) -> &pb::MethodDescriptorProto {
        &self.method
    }

    pub fn input_type(&self) -> Option<MessageDescriptor> {
        self.service
            .pool
            .find_relative_type(&self.service.name(), self.method.input_type())
            .and_then(|t| t.to_message())
    }

    pub fn output_type(&self) -> Option<MessageDescriptor> {
        self.service
            .pool
            .find_relative_type(&self.service.name(), self.method.output_type())
            .and_then(|t| t.to_message())
    }
}

#[derive(Clone)]
pub struct EnumDescriptor {
    pool: DescriptorPool,
    inner: Arc<EnumDescriptorInner>,
}

struct EnumDescriptorInner {
    name: TypeName,
    file_index: u32,
    proto: protobuf_descriptor::EnumDescriptorProto,
}

impl EnumDescriptor {
    pub fn name(&self) -> &str {
        &self.inner.name.name
    }

    pub fn file_index(&self) -> u32 {
        self.inner.file_index
    }

    pub fn proto(&self) -> &protobuf_descriptor::EnumDescriptorProto {
        &self.inner.proto
    }
}

impl EnumDescriptorInner {
    fn to_proto(&self) -> pb::EnumDescriptorProto {
        self.proto.clone()
    }
}

#[derive(Clone)]
pub struct FieldDescriptor {
    message: MessageDescriptor,
    field_index: usize,
}

impl FieldDescriptor {
    pub fn proto(&self) -> &pb::FieldDescriptorProto {
        &self.message.inner.proto.field()[self.field_index]
    }

    pub fn message(&self) -> &MessageDescriptor {
        &self.message
    }

    /// Assuming this field has a named type like an enum or message, this will
    /// get that type.
    pub fn find_type(&self) -> Option<TypeDescriptor> {
        self.message
            .pool
            .find_relative_type(&self.message.inner.name.name, self.proto().type_name())
    }
}

pub struct OneOfDescriptor {
    message: MessageDescriptor,
    index: usize,
}

impl OneOfDescriptor {
    pub fn proto(&self) -> &pb::OneofDescriptorProto {
        &self.message.inner.proto.oneof_decl()[self.index]
    }

    pub fn message(&self) -> &MessageDescriptor {
        &self.message
    }

    pub fn fields<'a>(&'a self) -> impl Iterator<Item = FieldDescriptor> + 'a {
        self.message.fields().filter(move |f| {
            f.proto().has_oneof_index() && f.proto().oneof_index() == (self.index as i32)
        })
    }

    pub fn index(&self) -> usize {
        self.index
    }
}

pub struct ExtendDescriptor {
    pool: DescriptorPool,
    inner: Arc<ExtendDescriptorInner>,
}

struct ExtendDescriptorInner {
    name: TypeName,
    file_index: u32,
    proto: protobuf_descriptor::FieldDescriptorProto,
}

impl ExtendDescriptor {
    pub fn name(&self) -> &str {
        &self.inner.name.name
    }

    pub fn proto(&self) -> &pb::FieldDescriptorProto {
        &self.inner.proto
    }
}

impl ExtensionTag for ExtendDescriptor {
    fn extension_number(&self) -> protobuf_core::ExtensionNumberType {
        self.inner.proto.number() as protobuf_core::ExtensionNumberType
    }

    fn extension_name(&self) -> protobuf_core::StringPtr {
        protobuf_core::StringPtr::Dynamic(self.inner.name.name.to_string())
    }

    fn default_extension_value(&self) -> protobuf_core::Value {
        // TODO: Deduplicate this more with DynamicMessage

        // TODO: Handle the default_value option.

        use pb::FieldDescriptorProto_Type::*;
        use protobuf_core::SingularValue;

        let is_repeated = self.proto().label() == pb::FieldDescriptorProto_Label::LABEL_REPEATED;

        // TODO: Only generate if needed.
        let default_value = match self.proto().typ() {
            TYPE_DOUBLE => SingularValue::Double(0.0),
            TYPE_FLOAT => SingularValue::Float(0.0),
            TYPE_INT64 => SingularValue::Int64(0),
            TYPE_UINT64 => SingularValue::UInt64(0),
            TYPE_INT32 => SingularValue::Int32(0),
            TYPE_FIXED64 => SingularValue::UInt64(0),
            TYPE_FIXED32 => SingularValue::UInt32(0),
            TYPE_BOOL => SingularValue::Bool(false),
            TYPE_STRING => SingularValue::String(String::new()),
            TYPE_GROUP => {
                todo!()
            }
            TYPE_MESSAGE | TYPE_ENUM => match self
                .pool
                .find_relative_type(&self.inner.name.name, self.proto().type_name())
            {
                Some(TypeDescriptor::Message(m)) => {
                    let val = DynamicMessage::new(m);
                    SingularValue::Message(Box::new(val))
                }
                Some(TypeDescriptor::Enum(e)) => {
                    let val = crate::message::DynamicEnum::new(e);
                    SingularValue::Enum(Box::new(val))
                }
                _ => {
                    todo!()

                    // return Err(
                    //     protobuf_core::WireError::BadDescriptor, /*
                    // err_msg("Unknown type in
                    //                                               * descriptor") */
                    // );
                }
            },
            TYPE_BYTES => SingularValue::Bytes(Vec::new().into()),
            TYPE_UINT32 => SingularValue::UInt32(0),
            TYPE_SFIXED32 => SingularValue::Int32(0),
            TYPE_SFIXED64 => SingularValue::Int64(0),
            TYPE_SINT32 => SingularValue::Int32(0),
            TYPE_SINT64 => SingularValue::Int64(0),
        };

        protobuf_core::Value::new(default_value, is_repeated)
    }
}

impl ExtendDescriptorInner {
    fn to_proto(&self) -> pb::FieldDescriptorProto {
        self.proto.clone()
    }
}
