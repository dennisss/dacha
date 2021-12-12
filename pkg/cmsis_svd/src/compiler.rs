use std::collections::HashMap;

use automata::regexp::vm::instance::RegExp;
use common::errors::*;
use common::failure::ResultExt;
use common::line_builder::LineBuilder;

use crate::helpers::*;
use crate::spec::*;

#[derive(Default)]
pub struct CompilerOptions {
    pub field_rewrites: Vec<FieldRewriteRule>,
}

pub struct FieldRewriteRule {
    pub register_name: RegExp,
    pub register_access: RegisterAccess,
    pub field_name: RegExp,
    pub new_name: String,
}

/*
Representing a field:
- register.field_mut() can return a struct of type FIELD_NAME_MUT {{ register: &'a mut REGISTER_NAME }}
*/


pub struct Compiler<'a> {
    options: &'a CompilerOptions,

    top_level_lines: LineBuilder,

    rewritten_fields: HashMap<&'a str, Field<'a>>,
}

impl<'a> Compiler<'a> {
    pub fn compile(input: &str, options: &CompilerOptions) -> Result<String> {
        let doc = xml::parse(input)?;

        let mut inst = Compiler {
            options,
            top_level_lines: LineBuilder::new(),
            rewritten_fields: HashMap::new(),
        };

        println!("{}", doc.root_element.name);

        let device = Device::parse_from(&doc.root_element)?;

        let mut peripherals_fields = LineBuilder::new();
        let mut peripherals_new = LineBuilder::new();
        let mut interrupt_variants = LineBuilder::new();
        let mut outer_lines = LineBuilder::new();

        let mut max_interrupt = 0;
        let mut seen_interrupts = HashMap::new();

        for peripheral in &device.peripherals {
            for interrupt in &peripheral.interrupts {
                if let Some(num) = seen_interrupts.insert(interrupt.name, interrupt.value) {
                    if num != interrupt.value {
                        return Err(err_msg("Duplicate interrupt with different number"));
                    }

                    continue;
                }

                interrupt_variants.add(format!("{} = {},", interrupt.name, interrupt.value));
                max_interrupt = std::cmp::max(max_interrupt, interrupt.value);
            }

            inst.compile_peripheral(
                &peripheral,
                &device.register_properties_group,
                &mut peripherals_fields,
                &mut peripherals_new,
                &mut outer_lines,
            )?;
        }

        let mut lines = LineBuilder::new();
        lines.add(format!(
            "
        pub trait AddressBlock {{
            fn base_address(&self) -> u32;

            fn offset(&self, offset: u32) -> OffsetAddressBlock<Self> where Self: Sized + Clone {{
                OffsetAddressBlock {{ base: self.clone() }}
            }}
        }}

        #[derive(Clone, Copy)]
        pub struct OffsetAddressBlock<Base: AddressBlock> {{
            // offset: u32,
            base: Base
        }}

        impl<Base: AddressBlock> AddressBlock for OffsetAddressBlock<Base> {{
            fn base_address(&self) -> u32 {{
                self.base.base_address() + 0
            }}
        }}

        pub trait RegisterRead {{
            type Value;

            fn read(&self) -> Self::Value;
        }}

        pub trait RegisterWrite {{
            type Value;

            fn write(&mut self, value: Self::Value);
        }}


        pub struct Peripherals {{
            hidden: (),
            {fields}
        }}

        impl Peripherals {{
            pub fn new() -> Self {{
                unsafe {{ Self {{
                    hidden: (),
                    {ctor}
                }} }}
            }}
        }}

        #[derive(Clone, Copy, PartialEq)]
        pub enum Interrupt {{
            {interrupt_variants}
        }}

        impl Interrupt {{
            pub const MAX: usize = {max_interrupt};
        }}

        ",
            fields = peripherals_fields.to_string(),
            ctor = peripherals_new.to_string(),
            interrupt_variants = interrupt_variants.to_string(),
            max_interrupt = max_interrupt
        ));

        lines.append(inst.top_level_lines);

        lines.append(outer_lines);

        Ok(lines.to_string())
    }

    fn compile_peripheral(
        &mut self,
        peripheral: &Peripheral<'a>,
        inherited_register_properties: &RegisterPropertiesGroup,
        peripherals_fields: &mut LineBuilder,
        peripherals_new: &mut LineBuilder,
        lines: &mut LineBuilder,
    ) -> Result<()> {
        println!("{:?}", peripheral.name);

        if let Some(name) = peripheral.derived_from {
            println!("- Derived From: {}", name);
        }

        println!("- Base: {:08x}", peripheral.base_address);

        // TOOD: If the registers is not present, then we should have a 'derivedFrom'
        // attribute.

        let peripheral_module = peripheral.name.to_ascii_lowercase();

        peripherals_fields.add(format!(
            "pub {mod_name}: {mod_name}::{name},",
            mod_name = peripheral_module,
            name = peripheral.name
        ));
        peripherals_new.add(format!(
            "{mod_name}: {mod_name}::{name}::new(),",
            mod_name = peripheral_module,
            name = peripheral.name
        ));

        lines.add(format!("pub mod {} {{", peripheral_module));
        lines.add("#[allow(unused_imports)] use super::*;");

        lines.indented(|lines| -> Result<()> {
            let mut peripheral_fields = LineBuilder::new();
            let mut peripheral_new = LineBuilder::new();
            let mut outer_lines = LineBuilder::new();

            let inherited_props = peripheral
                .register_properties_group
                .clone()
                .inherit(&peripheral.register_properties_group)
                .inherit(inherited_register_properties);

            let address_block_type = format!("{}_ADDRESS", peripheral.name);

            for register in &peripheral.registers {
                match register {
                    ClusterOrRegister::Cluster(cluster) => {
                        self.compile_cluster(
                            cluster,
                            &inherited_props,
                            &address_block_type,
                            &mut peripheral_new,
                            &mut peripheral_fields,
                            &mut outer_lines,
                        )?;
                    }
                    ClusterOrRegister::Register(register) => {
                        self.compile_register(
                            register,
                            &inherited_props,
                            &address_block_type,
                            &mut peripheral_new,
                            &mut peripheral_fields,
                            &mut outer_lines,
                        )?;
                    }
                }
            }

            lines.add(format!(
                "
                #[allow(non_camel_case_types)]
                pub struct {name} {{
                    hidden: (),
                    {peripheral_fields}    
                }}
        
                impl {name} {{
                    pub unsafe fn new() -> Self {{
                        let address_block = {address_block_type} {{ }};

                        Self {{
                            hidden: (),
                            {peripheral_new}
                        }}
                    }}
                }}

                #[derive(Clone, Copy)]
                pub struct {address_block_type} {{ }}

                impl AddressBlock for {address_block_type} {{
                    #[inline(always)]
                    fn base_address(&self) -> u32 {{ 0x{base_address:08x} }}
                }}

                ",
                name = peripheral.name,
                address_block_type = address_block_type,
                base_address = peripheral.base_address,
                peripheral_new = peripheral_new.to_string(),
                peripheral_fields = peripheral_fields.to_string()
            ));

            lines.nl();

            lines.append(outer_lines);

            Ok(())
        })?;

        lines.add("}");
        lines.nl();
        Ok(())
    }

    fn compile_cluster(
        &mut self,
        cluster: &Cluster<'a>,
        inherited_register_properties: &RegisterPropertiesGroup,
        upper_address_block_type: &str,
        peripheral_new: &mut LineBuilder,
        peripheral_fields: &mut LineBuilder,
        lines: &mut LineBuilder,
    ) -> Result<()> {
        let mut cluster_fields = LineBuilder::new();
        let mut cluster_new = LineBuilder::new();
        let mut outer_lines = LineBuilder::new();

        let cluster_name = {
            if cluster.dim_element_group.is_some() {
                cluster.name.strip_suffix("[%s]").ok_or_else(|| err_msg("Only array style dim groups are supported"))?
            } else {
                cluster.name
            }
        };

        let upper_address_block_type = {
            if cluster.dim_element_group.is_some() {
                format!("OffsetAddressBlock<super::{}>", upper_address_block_type)
            } else {
                format!("super::{}", upper_address_block_type)
            }
        };

        let field_name = cluster_name.to_ascii_lowercase();
        let mod_name = cluster_name.to_ascii_lowercase();

        if let Some(dim_element_group) = &cluster.dim_element_group {
            peripheral_fields.add(format!(
                "/// {desc}
                pub {field_name}: [{mod_name}::{name}; {dim}],",
                field_name = field_name,
                name = cluster_name,
                dim = dim_element_group.dim,
                desc = cluster.description.replace("\n", " "),
                mod_name = mod_name
            ));
            
            let mut instances = LineBuilder::new();
            for i in 0..dim_element_group.dim {
                instances.add(format!("{mod_name}::{name}::new(address_block.offset({increment} * {i})),",
                    mod_name = mod_name,
                    name = cluster_name,
                    increment = dim_element_group.dim_increment,
                    i = i
                ));
            }

            peripheral_new.add(format!(
                "{field_name}: [{instances}],",
                instances = instances.to_string(),
                field_name = field_name
            ));

        } else {
            peripheral_fields.add(format!(
                "pub {field_name}: {mod_name}::{name},",
                field_name = field_name,
                name = cluster.name,
                mod_name = mod_name
            ));
    
            peripheral_new.add(format!(
                "{field_name}: {mod_name}::{name}::new(address_block),",
                field_name = field_name,
                name = cluster.name,
                mod_name = mod_name
            ));
        }

        let address_block_type = format!("{}_ADDRESS", cluster_name);

        for register in &cluster.children {
            match register {
                ClusterOrRegister::Cluster(cluster) => {
                    self.compile_cluster(
                        cluster,
                        inherited_register_properties,
                        &address_block_type,
                        &mut cluster_new,
                        &mut cluster_fields,
                        &mut outer_lines,
                    )?;
                }
                ClusterOrRegister::Register(register) => {
                    self.compile_register(
                        register,
                        inherited_register_properties,
                        &address_block_type,
                        &mut cluster_new,
                        &mut cluster_fields,
                        &mut outer_lines,
                    )?;
                }
            }
        }

        // TODO: Gurantee that no registers are named 'address_block'. Otherwise this
        // will conflict.
        lines.add(format!(
            "pub mod {mod_name} {{
                #[allow(unused_imports)] use super::*;

                /// {desc}
                pub struct {name} {{
                    hidden: (),
                    {cluster_fields}
                }}

                impl {name} {{
                    pub unsafe fn new(address_block: {upper_address_block_type}) -> Self {{
                        let address_block = {address_block_type} {{
                            parent: address_block
                        }};

                        Self {{
                            hidden: (),
                            {cluster_new}
                        }}
                    }}
                }}

                #[derive(Clone, Copy)]
                pub struct {address_block_type} {{
                    parent: {upper_address_block_type}
                }}

                impl AddressBlock for {address_block_type} {{
                    #[inline(always)]
                    fn base_address(&self) -> u32 {{
                        self.parent.base_address() + {address_off}
                    }}
                }}

                {outer_lines}
            }}
            ",
            mod_name = mod_name,
            name = cluster_name,
            address_block_type = address_block_type,
            desc = cluster.description.replace("\n", " "),
            cluster_fields = cluster_fields.to_string(),
            cluster_new = cluster_new.to_string(),
            upper_address_block_type = upper_address_block_type,
            address_off = cluster.address_offset,
            outer_lines = outer_lines.to_string()
        ));

        Ok(())
    }

    // TODO: Verify that all registers in a peripheral have a unique name (even if
    // in a cluster?)
    fn compile_register(
        &mut self,
        register: &Register<'a>,
        inherited_register_properties: &RegisterPropertiesGroup,
        address_block_type: &str,
        peripheral_new: &mut LineBuilder,
        peripheral_fields: &mut LineBuilder,
        lines: &mut LineBuilder,
    ) -> Result<()> {
        let properties = register
            .properties
            .clone()
            .inherit(inherited_register_properties)
            .resolve()
            .with_context(|e| format!("While resolving {}: {}", register.name, e))?;

        // Size of the register in bits
        if properties.size != 32 {
            return Err(err_msg("Register is not 32 bits in size"));
        }

        // TODO: Support registers with <readAction>modifyExternal</readAction>
        // ^ This means that reading should require a mutable lock.
        // If we use the same register type for each pin, then that means we can't use 

        let register_name = {
            if register.dim_element_group.is_some() {
                register.name.strip_suffix("[%s]").ok_or_else(|| err_msg("Only array style dim groups are supported"))?
            } else {
                register.name
            }
        };

        let address_block_type = {
            if register.dim_element_group.is_some() {
                format!("OffsetAddressBlock<super::{}>", address_block_type)
            } else {
                format!("super::{}", address_block_type)
            }
        };

        println!("  - {}", register_name);

        // Must check "resetValue" and "resetMask".

        let register_mod = escape_keyword(&register_name.to_ascii_lowercase());

        if let Some(dim_element_group) = &register.dim_element_group {
            peripheral_fields.add(format!(
                "/// {desc}
                    pub {mod_name}: [{mod_name}::{name}; {dim}],",
                name = register_name,
                dim = dim_element_group.dim,
                desc = register.description.replace("\n", " "),
                mod_name = register_mod
            ));
            
            let mut instances = LineBuilder::new();
            for i in 0..dim_element_group.dim {
                instances.add(format!("{mod_name}::{name}::new(address_block.offset({increment} * {i})),",
                    mod_name = register_mod,
                    name = register_name,
                    increment = dim_element_group.dim_increment,
                    i = i
                ));
            }

            peripheral_new.add(format!(
                "{mod_name}: [{instances}],",
                instances = instances.to_string(),
                mod_name = register_mod
            ));

        } else {
            peripheral_fields.add(format!(
                "/// {desc}
                    pub {mod_name}: {mod_name}::{name},",
                name = register_name,
                desc = register.description.replace("\n", " "),
                mod_name = register_mod
            ));
    
            peripheral_new.add(format!(
                "{mod_name}: {mod_name}::{name}::new(address_block),",
                name = register_name,
                mod_name = register_mod
            ));
        }

        lines.add(format!("pub mod {} {{", register_mod));
        lines.add("#[allow(unused_imports)] use super::*;");

        lines.indented(|lines| -> Result<()> {
            let mut read_value_impl = LineBuilder::new();
            let mut write_value_impl = LineBuilder::new();
            let mut outer_lines = LineBuilder::new();

            let mut collapse_field = false;
            let mut last_field = None;

            let mut same_read_write_values = properties.access == RegisterAccess::ReadWrite;

            for field in &register.fields {
                let compiled = self.compile_field(
                    field,
                    &register,
                    &properties,
                    &mut read_value_impl,
                    &mut write_value_impl,
                    &mut outer_lines,
                )?;

                if field.read_enumerated_values != field.write_enumerated_values {
                    same_read_write_values = false;
                }

                if register.fields.len() == 1 && &compiled.name == register_name {
                    last_field = Some(compiled);
                    collapse_field = true;
                }
            }

            let mut value_created = false;

            let read_value_type = if same_read_write_values {
                format!("{}_VALUE", register_name)
            } else {
                format!("{}_READ_VALUE", register_name)
            };

            let write_value_type = if same_read_write_values {
                format!("{}_VALUE", register_name)
            } else {
                format!("{}_WRITE_VALUE", register_name)
            };


            // TODO: When read and write segments are the same, 

            let mut associated_types = LineBuilder::new();
            let mut value_lines = LineBuilder::new();
            let mut register_impl = LineBuilder::new();
            let mut register_extra_impls = LineBuilder::new();

            if properties.access.can_read() && collapse_field {
                let compiled_field = last_field.as_ref().unwrap();
                
                register_extra_impls.add(format!(
                    "
                    impl RegisterRead for {name} {{
                        type Value = {typ};

                        #[inline(always)]
                        fn read(&self) -> Self::Value {{
                            let raw = unsafe {{ ::core::ptr::read_volatile(self.ptr()) }};
                            {reader}
                        }}
                    }}
                    ",
                    name = register_name,
                    typ = compiled_field.read_inner_type,
                    reader = compiled_field.reader
                ));
            } else if properties.access.can_read() {
                // associated_types.add("pub type Read = ReadValue;");

                register_extra_impls.add(format!(
                    "
                    impl RegisterRead for {name} {{
                        type Value = {read_value_type};

                        #[inline(always)]
                        fn read(&self) -> Self::Value {{
                            let v = unsafe {{ ::core::ptr::read_volatile(self.ptr()) }};
                            {read_value_type}::from_raw(v)
                        }}
                    }}
                    ",
                    name = register_name,
                    read_value_type = read_value_type
                ));

                value_lines.add(format!(
                    "
                    #[derive(Clone, Copy, PartialEq)]
                    pub struct {read_value_type} {{ raw: u32 }}
        
                    impl {read_value_type} {{
                        pub fn new() -> Self {{ Self {{ raw: 0 }} }}
    
                        #[inline(always)]
                        pub fn from_raw(raw: u32) -> Self {{ Self {{ raw }} }}
        
                        #[inline(always)]
                        pub fn to_raw(&self) -> u32 {{ self.raw }}
        
                        {read_value_impl}
                    }}    
                    ",
                    read_value_type = read_value_type,
                    read_value_impl = read_value_impl.to_string(),
                ));
                value_created = true;
            } else {
                // TODO: Verify no readable fields?
            }

            if collapse_field && properties.access.can_write() {
                let compiled_field = last_field.as_ref().unwrap();

                register_extra_impls.add(format!(
                    "
                    impl RegisterWrite for {name} {{
                        type Value = {typ};

                        #[inline(always)]
                        fn write(&mut self, value: Self::Value) {{
                            let old_raw = 0;
                            let raw = {writer};
                            unsafe {{ ::core::ptr::write_volatile(self.ptr(), raw) }};
                        }}
                    }}
                    ",
                    name = register_name,
                    typ = compiled_field.write_inner_type,
                    writer = compiled_field.writer
                ));

                // If we just have a single field which uses enumerated values, add accessors to directly set each value.
                if register.fields[0].write_enumerated_values.is_some() {
                    let compiled_field = last_field.as_ref().unwrap();

                    for value in register.fields[0].write_enumerated_values.as_ref().unwrap() {
                        register_impl.add(format!(
                            "
                            pub fn write_{value_name}(&mut self) {{
                                self.write({enum_type}::{value_variant})
                            }}
                            ",
                            value_name = value.name.to_ascii_lowercase(),
                            enum_type = compiled_field.write_inner_type,
                            value_variant = escape_keyword(value.name),
                        ));                               
                    }
                }


            } else if properties.access.can_write() {
                // associated_types.add("pub type Write = WriteValue;");

                /*
                Register generates struct named: REGISTER
                Which can read a value named REGISTER_W_VALUE
                */

                register_extra_impls.add(format!(
                    "
                    impl RegisterWrite for {name} {{
                        type Value = {write_value_type};

                        #[inline(always)]
                        fn write(&mut self, value: Self::Value) {{
                            unsafe {{ ::core::ptr::write_volatile(self.ptr(), value.to_raw()) }}
                        }}
                    }}
                    ",
                    name = register_name,
                    write_value_type = write_value_type,
                ));

                // NOTE: The return value of write_with is mainly for convenience.
                register_impl.add(format!(
                    "
                    pub fn write_with<F: Fn(&mut {write_value_type}) -> &mut {write_value_type}>(&mut self, f: F) {{
                        let mut v = {write_value_type}::new();
                        f(&mut v);
                        self.write(v);
                    }}
                    ",
                    write_value_type = write_value_type,
                ));

                if !same_read_write_values || !value_created {
                    value_lines.add(format!(
                        "
                        #[derive(Clone, Copy, PartialEq)]
                        pub struct {write_value_type} {{ raw: u32 }}
            
                        impl {write_value_type} {{
                            pub fn new() -> Self {{ Self {{ raw: 0 }} }}
    
                            #[inline(always)]
                            pub fn from_raw(raw: u32) -> Self {{ Self {{ raw }} }}
            
                            #[inline(always)]
                            pub fn to_raw(&self) -> u32 {{ self.raw }}
    
                            {write_value_impl}
                        }}  
                        ",
                        write_value_type = write_value_type,
                        write_value_impl = write_value_impl.to_string(),
                    ));
                }
            }

            // TODO: Make to_raw/from_raw unsafe?
            lines.add(format!(
                "
            #[allow(non_camel_case_types)]
            pub struct {name} {{ address_block: {address_block_type} }}
            
            impl {name} {{
                pub unsafe fn new(address_block: {address_block_type}) -> Self {{
                    Self {{ address_block }}
                }}

                #[inline(always)]
                fn ptr(&self) -> *mut u32 {{
                    (self.address_block.base_address() + 0x{address_off:08x}) as *mut u32
                }}

                {register_impl}
            }}
            
            {register_extra_impls}

            ",
                name = register_name,
                address_block_type = address_block_type,
                address_off = register.address_off,
                register_impl = register_impl.to_string(),
                register_extra_impls = register_extra_impls.to_string()
            ));

            lines.nl();
            lines.append(value_lines);
            lines.append(outer_lines);
            lines.nl();

            Ok(())
        })?;

        lines.add("}");
        lines.nl();

        // "name"
        // "addressOffset"

        // Need to look up the "size" to get the number of bits in the
        // register ^ For now shoudl always be 32

        // Maybe has a derivedFrom

        /*
        If "dim" is present, then we have an array field with a "%s" in the name (e.g. EVENTS_COMPARE[%s])
        <dim>0x4</dim>
        <dimIncrement>0x4</dimIncrement>
        */

        Ok(())
    }

    fn compile_field(
        &mut self,
        field: &Field<'a>,
        register: &Register,
        register_props: &ResolvedRegisterPropertiesGroup,
        read_value_impl: &mut LineBuilder,
        write_value_impl: &mut LineBuilder,
        outer_lines: &mut LineBuilder,
    ) -> Result<CompiledField> {
        let num_bits = field.msb - field.lsb + 1;

        println!("    - {}: [{}, {}]", field.name, field.lsb, field.msb);

        let accessor_name = field.name.to_ascii_lowercase();

        let mask: u64 = ((1 << num_bits) - 1) << field.lsb;

        let read_raw_value = format!(
            "(raw & 0x{mask:08x}) >> {lsb}",
            lsb = field.lsb,
            mask = mask,
        );

        let write_raw_value = format!(
            "(old_raw & !0x{mask:08x}) | (value << {lsb})",
            mask = mask,
            lsb = field.lsb
        );

        // let use_primitive =
        //     !field.read_enumerated_values.is_some() &&
        // !field.write_enumerated_values.is_some();

        let mut same_read_write_types = field.read_enumerated_values
            == field.write_enumerated_values
            || !register_props.access.can_read()
            || !register_props.access.can_write();

        let mut read_name_override = None;
        let mut write_name_override = None;
        for rule in &self.options.field_rewrites {
            if rule.register_name.test(register.name) && rule.field_name.test(field.name) {
                println!("     MATCH!!");
                if rule.register_access.can_read() {
                    read_name_override = Some(rule.new_name.as_str());
                }
                if rule.register_access.can_write() {
                    write_name_override = Some(rule.new_name.as_str());
                }
            }
        }

        let read_inner_type = if field.read_enumerated_values.is_none() {
            "u32".to_string()
        } else {
            if let Some(over) = read_name_override.as_ref() {
                over.to_string()
            } else if same_read_write_types {
                format!("{}_FIELD", field.name)
            } else {
                format!("{}_READ_FIELD", field.name)
            }
        };

        //
        // - Should have independent read and write types
        // - If read enumerated values
        //

        let compiled_field = CompiledField {
            name: field.name.to_string(),
            read_inner_type: read_inner_type.clone(),
            write_inner_type: if field.write_enumerated_values.is_none() {
                "u32".to_string()
            } else {
                if let Some(over) = write_name_override.as_ref() {
                    over.to_string()
                } else if same_read_write_types {
                    format!("{}_FIELD", field.name)
                } else {
                    format!("{}_WRITE_FIELD", field.name)
                }
            },
            reader: if field.read_enumerated_values.is_none() {
                read_raw_value.clone()
            } else {
                format!(
                    "{enum_name}::from_value({value})",
                    enum_name = read_inner_type,
                    value = read_raw_value
                )
            },
            writer: if field.write_enumerated_values.is_none() {
                write_raw_value.clone()
            } else {
                "value.to_value()".to_string()
            },
        };

        let make_enum_accessors = |enum_name: &str, values: &[EnumeratedValue]| {
            if values.len() == 1 {
                return format!(
                    "
                    pub fn set_{accessor_name}(&mut self) -> &mut Self {{
                        let value = {enum_name}::{variant_name}.to_value();
                        let old_raw = self.raw;
                        self.raw = {write_value};
                        self   
                    }}
                    ",
                    accessor_name = accessor_name,
                    enum_name = enum_name,
                    write_value = write_raw_value,
                    variant_name = escape_keyword(values[0].name)
                );
            }
            
            format!(
                "pub fn {escaped_accessor_name}(&self) -> {enum_name} {{
                    let raw = self.raw;
                    {enum_name}::from_value({value})
                }}
                
                pub fn set_{accessor_name}(&mut self, value: {enum_name}) -> &mut Self {{
                    let value = value.to_value();
                    let old_raw = self.raw;
                    self.raw = {write_value};
                    self
                }}

                pub fn set_{accessor_name}_with<F: Fn(&mut {enum_name}) -> &mut {enum_name}>(&mut self, f: F) -> &mut Self {{
                    let mut value = self.{escaped_accessor_name}();
                    f(&mut value);
                    self.set_{accessor_name}(value)
                }}
                
            ",
                accessor_name = accessor_name,
                escaped_accessor_name = escape_keyword(&accessor_name),
                enum_name = enum_name,
                value = read_raw_value,
                write_value = write_raw_value
            )
        };

        

        let mut struct_added = false;

        if let Some(ref read_enum_values) = field.read_enumerated_values {
            if register_props.access.can_read() {
                let enum_name = &compiled_field.read_inner_type;

                read_value_impl.add(make_enum_accessors(&enum_name, &read_enum_values));

                // TODO: Validate that the name and old fields have identical formats.
                if read_name_override.is_some() {
                    if !self.rewritten_fields.contains_key(enum_name.as_str()) {
                        Self::compile_enumerated_values(
                            &enum_name,
                            &read_enum_values,
                            &mut self.top_level_lines,
                        )?;

                        self.rewritten_fields
                            .insert(read_name_override.as_ref().unwrap(), field.clone());
                    }
                } else {
                    Self::compile_enumerated_values(&enum_name, &read_enum_values, outer_lines)?;
                    struct_added = true;
                }
            }
        } else {
            let accessors = format!(
                "pub fn {escaped_accessor_name}(&self) -> u32 {{
                    let raw = self.raw;
                    {value}
                }}

                pub fn set_{accessor_name}(&mut self, value: u32) -> &mut Self {{
                    let old_raw = self.raw;
                    self.raw = {write_raw_value};
                    self
                }}
            ",
                accessor_name = accessor_name,
                escaped_accessor_name = escape_keyword(&accessor_name),
                value = read_raw_value,
                write_raw_value = write_raw_value
            );

            read_value_impl.add(&accessors);
        }

        /*
        Assuming we know how many bits an enumerated value has, we can do ReservedX to block it from being commended from being used.

        */

        if let Some(ref write_enumerated_values) = field.write_enumerated_values {
            if register_props.access.can_write() {
                let enum_name = &compiled_field.write_inner_type;
                write_value_impl.add(make_enum_accessors(&enum_name, &write_enumerated_values));

                if write_name_override.is_some() {
                    if !self.rewritten_fields.contains_key(enum_name.as_str()) {
                        Self::compile_enumerated_values(
                            &enum_name,
                            &write_enumerated_values,
                            &mut self.top_level_lines,
                        )?;

                        self.rewritten_fields
                            .insert(write_name_override.as_ref().unwrap(), field.clone());
                    }
                } else if !same_read_write_types || !struct_added {
                    Self::compile_enumerated_values(
                        &enum_name,
                        &write_enumerated_values,
                        outer_lines,
                    )?;
                }
            }
        } else {
            let accessors = format!(
                "pub fn set_{accessor_name}(&mut self, value: u32) -> &mut Self {{
                    let old_raw = self.raw;
                    self.raw = {write_value};
                    self
                }}
            ",
                accessor_name = accessor_name,
                write_value = write_raw_value
            );

            write_value_impl.add(&accessors);
        }

        Ok(compiled_field)
    }

    fn compile_enumerated_values(
        enum_name: &str,
        values: &[EnumeratedValue],
        lines: &mut LineBuilder,
    ) -> Result<()> {
        let mut variants = LineBuilder::new();
        let mut fields = LineBuilder::new();

        for (i, value) in values.iter().enumerate() {
            let escaped_name = escape_keyword(value.name);

            variants.add(format!(
                "// {}
                {} = {}{}",
                value.desc.replace("\n", " "),
                escaped_name,
                value.value,
                if i == values.len() - 1 { "" } else { "," }
            ));

            fields.add(format!(
                "
                pub fn is_{name}(&self) -> bool {{
                    *self == Self::{escaped_name}
                }}

                pub fn set_{name}(&mut self) -> &mut Self {{
                    *self = Self::{escaped_name};
                    self
                }}
                ",
                name = value.name.to_lowercase(),
                escaped_name = escaped_name
            ));
        }

        lines.add(format!(
            "
            enum_def_with_unknown!(#[allow(non_camel_case_types)] {name} u32 =>
                {variants}
            );
    
            impl {name} {{
                {fields}
            }}
            ",
            name = enum_name,
            variants = variants.to_string(),
            fields = fields.to_string()
        ));

        Ok(())
    }
}

struct CompiledField {
    name: String,
    read_inner_type: String,
    write_inner_type: String,
    reader: String,
    writer: String,
}
