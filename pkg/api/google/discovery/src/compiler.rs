use std::collections::{BTreeMap, HashMap};

use common::{errors::*, line_builder::LineBuilder};
use http::uri::Uri;
use parsing::ascii::AsciiString;

use crate::format::*;

pub struct Compiler<'a> {
    lines: LineBuilder,
    desc: &'a RestDescription,
}

impl<'a> Compiler<'a> {
    pub fn compile(desc: &'a RestDescription) -> Result<String> {
        let mut c = Self {
            lines: LineBuilder::new(),
            desc,
        };

        for (name, s) in &desc.schemas {
            if s.typ != Some("object".to_string()) {
                return Err(err_msg("All top level schemas should be objects"));
            }

            c.compile_field_type(&name, s, false, false)?;
        }

        let mut methods = LineBuilder::new();
        for (resource_name, resource) in &desc.resources {
            c.compile_resource(resource_name.as_str(), resource, &mut methods)?;
        }

        let name = desc.name.clone().or(desc.canonicalName.clone()).unwrap();
        let name = common::snake_to_camel_case(&name);

        // desc.baseUrl

        c.lines.add(format!(
            r#"
            pub struct {name}Client {{
                rest_client: Arc<::google_auth::GoogleRestClient>
            }}

            impl {name}Client {{
                const BASE_URL: &'static str = "{base_url}";
                
                pub fn new(rest_client: Arc<::google_auth::GoogleRestClient>) -> Result<Self> {{
                    Ok(Self {{ rest_client }})
                }}

                {methods}
            }}
            "#,
            name = &name,
            base_url = &desc.baseUrl,
            methods = methods.to_string()
        ));

        Ok(c.lines.to_string())
    }

    fn compile_resource(
        &mut self,
        resource_name: &str,
        resource: &RestResource,
        methods_output: &mut LineBuilder,
    ) -> Result<()> {
        let methods = match &resource.methods {
            Some(v) => v,
            None => return Ok(()),
        };

        let resource_name = common::camel_to_snake_case(resource_name);

        for (method_name, method) in methods {
            let method_name = common::camel_to_snake_case(&method_name);

            let mut params = BTreeMap::new();
            if let Some(pmap) = &method.parameters {
                for (name, value) in pmap {
                    params.insert(name.clone(), value);
                }
            }

            let mut method_required_args = String::new();
            let mut method_request_args = String::new();
            let mut method_params_args = String::new();

            let mut path_args = String::new();
            let mut query_building = String::new();

            if let Some(param_order) = &method.parameterOrder {
                for name in param_order {
                    let schema = params
                        .remove(name)
                        .ok_or_else(|| err_msg("Missing parameter in parameterOrder"))?;
                    if schema.required != Some(true) {
                        return Err(err_msg("Only required params should be in the order"));
                    }

                    let arg_name = self.to_rust_identifier(&name);

                    let mut schema_name = format!(
                        "{}{}{}",
                        common::snake_to_camel_case(&resource_name),
                        common::snake_to_camel_case(&method_name),
                        common::snake_to_camel_case(name)
                    );

                    let typ = self.compile_field_type(&schema_name, schema, true, false)?;

                    method_required_args.push_str(&format!(", {}: {}", arg_name, typ));

                    let location = schema
                        .location
                        .as_ref()
                        .map(|s| s.as_str())
                        .ok_or_else(|| err_msg("Required parameter has undefined location"))?;
                    match location {
                        "path" => {
                            path_args.push_str(&format!(", {name} = {name}", name = arg_name));
                        }
                        "query" => {
                            // TODO: Assumes that only string parameters are used.
                            query_building.push_str(&format!(
                                "query_builder.add(b\"{name}\", {arg_name}.as_bytes());\n",
                                name = name,
                                arg_name = arg_name,
                            ));
                        }
                        _ => {
                            return Err(format_err!(
                                "Unsupported parameter location: {}",
                                location
                            ));
                        }
                    }
                }
            }

            let mut request_ref = "&()".to_string();
            if let Some(request) = &method.request {
                method_request_args.push_str(&format!(", request: &{}", &request.typ_reference));
                request_ref = format!("request");
            }

            if !params.is_empty() {
                for p in params.values() {
                    if p.location.as_ref().map(|s| s.as_str()) != Some("query") {
                        return Err(err_msg("Expected all extra parameters to be in the query"));
                    }
                }

                let mut schema_name = format!(
                    "{}{}Parameters",
                    common::snake_to_camel_case(&resource_name),
                    common::snake_to_camel_case(&method_name),
                );

                self.compile_object(&schema_name, &params)?;

                method_params_args.push_str(&format!(", parameters: &{}", schema_name));

                query_building
                    .push_str(&format!("parameters.serialize_to(&mut query_builder)?;\n"));
            }

            let mut return_typ = "()".to_string();

            if let Some(response) = &method.response {
                return_typ = response.typ_reference.clone();
            }

            methods_output.add(format!(
                r#"
                pub async fn {resource_name}_{method_name}(&self{args}) -> Result<{return_typ}> {{
                    let mut query_builder = http::query::QueryParamsBuilder::new();
                    {query_building}
                    self.rest_client.request_json::<_, {return_typ}>(
                        ::http::Method::{http_method},
                        &format!({path}{path_args}),
                        query_builder.build().as_str(),
                        {request_ref}
                    ).await
                }}
                "#,
                resource_name = resource_name,
                method_name = method_name,
                args = format!(
                    "{}{}{}",
                    method_required_args, method_request_args, method_params_args
                ),
                return_typ = return_typ,
                http_method = method.httpMethod,
                path = self.path_format_string(&method.path, false)?,
                path_args = path_args,
                request_ref = request_ref,
                query_building = query_building,
            ));

            if method.supportsMediaUpload {
                let media_upload = method
                    .mediaUpload
                    .as_ref()
                    .ok_or_else(|| err_msg("Missing mediaUpload with supportsMediaUpload"))?;

                methods_output.add(format!(
                    r#"
                    pub async fn {resource_name}_{method_name}_with_upload(
                        &self{args}, data: Box<dyn http::Body>
                    ) -> Result<{return_typ}> {{
                        let mut query_builder = http::query::QueryParamsBuilder::new();
                        {query_building}

                        // TODO: Clear the old value.
                        let content_type = request.contentType.clone();

                        self.rest_client.request_upload::<_, {return_typ}>(
                            ::http::Method::{http_method},
                            &format!({simple_path}{path_args}),
                            &format!({resumable_path}{path_args}),
                            query_builder,
                            &content_type,
                            {request_ref},
                            data
                        ).await
                    }}
                    "#,
                    resource_name = resource_name,
                    method_name = method_name,
                    args = format!(
                        "{}{}{}",
                        method_required_args, method_request_args, method_params_args
                    ),
                    return_typ = return_typ,
                    http_method = method.httpMethod,
                    simple_path =
                        self.path_format_string(&media_upload.protocols.simple.path, false)?,
                    resumable_path =
                        self.path_format_string(&media_upload.protocols.resumable.path, false)?,
                    path_args = path_args,
                    request_ref = request_ref,
                    query_building = query_building,
                ));
            }

            if method.supportsMediaDownload {
                methods_output.add(format!(
                    r#"
                    pub async fn {resource_name}_{method_name}_download(&self{args}) -> Result<Box<dyn http::Body>> {{
                        let mut query_builder = http::query::QueryParamsBuilder::new();
                        query_builder.add(b"alt", b"media");
                        {query_building}
                        self.rest_client.request_download(
                            ::http::Method::{http_method},
                            &format!({path}{path_args}),
                            query_builder.build().as_str(),
                            {request_ref}
                        ).await
                    }}
                    "#,
                    resource_name = resource_name,
                    method_name = method_name,
                    args = format!(
                        "{}{}{}",
                        method_required_args, method_request_args, method_params_args
                    ),
                    http_method = method.httpMethod,
                    path = self.path_format_string(&method.path, method.useMediaDownloadService)?,
                    path_args = path_args,
                    request_ref = request_ref,
                    query_building = query_building,
                ));
            }

            // "supportsMediaDownload": true,
            // "useMediaDownloadService": true

            /*
                    objects_insert_with_upload(required_params, Body, Params)
                    // request: &Object,

                    Multi-part is defined in
                        - https://datatracker.ietf.org/doc/html/rfc1521#section-7.2.1

                    multipart/related
                    - https://datatracker.ietf.org/doc/html/rfc2387

            "https://storage.googleapis.com/upload/storage/v1/b/BUCKET_NAME/o?uploadType=media&name=OBJECT_NAME"


                    Boundary must be <= 70 characters
                    */
        }

        Ok(())
    }

    fn path_format_string(&self, mut path: &str, use_download_service: bool) -> Result<String> {
        let mut out = String::new();

        out.push('"');

        let mut base_url = self.desc.baseUrl.parse::<Uri>()?;
        if use_download_service {
            base_url.path = AsciiString::new(&format!("/download{}", base_url.path.as_str()));
        }

        // NOTE: 'path' can't be parsed as a regular Uri since it contains invalid
        // '{'/'}' characters.
        let absolute_url = {
            if path.starts_with("/") {
                let mut url = base_url.clone();
                url.path = AsciiString::new("");

                format!("{}{}", url.to_string()?, path)
            } else {
                // TODO: Verify baseUrl ends in a '/'
                format!("{}{}", base_url.to_string()?, path)
            }
        };

        let mut path = absolute_url.as_str();
        loop {
            match path.split_once('{') {
                Some((pre, post)) => {
                    out.push_str(pre);

                    let (ident, rest) = post.split_once('}').unwrap();
                    path = rest;

                    let ident = ident.strip_prefix('+').unwrap_or(ident);
                    let ident = self.to_rust_identifier(ident);

                    out.push_str(&format!("{{{}}}", ident));
                }
                None => {
                    out.push_str(path);
                    break;
                }
            }
        }

        out.push('"');

        Ok(out)
    }

    /// Returns the Rust type that can be used to store the given schema.
    fn compile_field_type(
        &mut self,
        schema_name: &str,
        schema: &JsonSchema,
        get_reference: bool,
        array_item: bool,
    ) -> Result<String> {
        let mut is_object = false;
        let mut typ = match schema.typ.as_ref().map(|s| s.as_str()) {
            Some("string") => {
                if get_reference {
                    "str".to_string()
                } else {
                    "String".to_string()
                }
            }
            Some("object") => {
                is_object = true;

                let mut props = BTreeMap::new();
                if let Some(p) = &schema.properties {
                    for (k, v) in p {
                        props.insert(k.clone(), v);
                    }
                }

                self.compile_object(schema_name, &props)?;

                schema_name.to_string()
            }
            Some("array") => {
                let items = schema
                    .items
                    .as_ref()
                    .ok_or_else(|| err_msg("Array types must specify an item type"))?;

                let item_typ =
                    self.compile_field_type(&format!("{}Item", schema_name), &items, false, true)?;

                if get_reference {
                    format!("[{}]", item_typ)
                } else {
                    format!("Vec<{}>", item_typ)
                }
            }
            Some("boolean") => "bool".to_string(),
            Some("integer") | Some("number") => {
                let format = schema
                    .format
                    .as_ref()
                    .ok_or_else(|| err_msg("Integer type missing format"))?
                    .as_str();

                match format {
                    "int64" => "i64",
                    "int32" => "i32",
                    "uint32" => "u32",
                    "double" => "f32",
                    _ => return Err(format_err!("Unknown integer format: {}", format)),
                }
                .to_string()
            }
            Some(v) => {
                return Err(format_err!("Unsupported type: {}", v));
            }
            None => {
                is_object = true;

                let typ_ref = schema
                    .typ_reference
                    .as_ref()
                    .ok_or_else(|| err_msg("Expected either type or $ref in schema"))?;

                typ_ref.clone()
            }
        };

        // TODO: Handle 'schema.repeated'.

        if is_object && !array_item {
            typ = format!("Option<{}>", typ);
        }

        if get_reference {
            typ = format!("&{}", typ);
        }

        Ok(typ)
    }

    fn compile_object(
        &mut self,
        schema_name: &str,
        properties: &BTreeMap<String, &JsonSchema>,
    ) -> Result<()> {
        let mut fields = LineBuilder::new();

        for (prop_name, prop) in properties {
            let inner_name = format!(
                "{}{}",
                schema_name,
                common::snake_to_camel_case(&common::camel_to_snake_case(prop_name))
            );

            let field_name = self.to_rust_identifier(&prop_name);

            // We assume that
            fields.add(format!(
                r#"#[parse(sparse = true, name = "{}")]"#,
                prop_name
            ));
            fields.add(format!(
                "pub {}: {},",
                field_name,
                self.compile_field_type(&inner_name, prop, false, false)?
            ));
        }

        // TODO: Validate fields that are marked with 'schema.required'.

        self.lines.add(&format!(
            r#"
            #[derive(Parseable, Debug, Default)]
            #[parse(allow_unknown = true)]
            pub struct {name} {{
                {fields}
            }}
            "#,
            name = schema_name,
            fields = fields.to_string()
        ));

        Ok(())
    }

    fn to_rust_identifier(&self, ident: &str) -> String {
        if ident == "type" {
            return "typ".to_string();
        }

        ident.to_string()
    }
}
