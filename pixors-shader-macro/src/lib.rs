use proc_macro::TokenStream;
use quote::{format_ident, quote};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use syn::{
    DeriveInput, Ident, LitInt, LitStr, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
};

// ── parsed attributes ───────────────────────────────────────────────────────

struct KernelArgs {
    source: String,
    entry: String,
    body_fn: String,
    includes: Vec<String>,
    specs: Vec<Specialization>,
    inputs: Vec<String>,
    output: String,
    workgroup: (u32, u32, u32),
    dispatch: DispatchKind,
    class: ClassKind,
}

struct Specialization {
    id: String,
    types: Vec<Ident>,
    formats: Vec<Ident>,
}

enum DispatchKind {
    PerPixel,
}

enum ClassKind {
    PerPixel,
}

// ── codec → format mapping (for auto-derive) ────────────────────────────────

fn codec_formats(codec: &str) -> Vec<&'static str> {
    match codec {
        "U8Codec" => vec!["Rgba8", "Rgb8", "Gray8", "GrayA8", "Cmyk8"],
        "U16Codec" => vec!["Rgba16", "Rgb16", "Gray16", "GrayA16"],
        "F16Codec" => vec!["RgbaF16", "RgbF16"],
        "F32Codec" => vec!["RgbaF32", "RgbF32", "GrayF32"],
        _ => panic!("unknown codec: {codec}"),
    }
}

// ── custom Parse ────────────────────────────────────────────────────────────

impl Parse for KernelArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut source: Option<String> = None;
        let mut entry: Option<String> = None;
        let mut body_fn: Option<String> = None;
        let mut specs: Vec<Specialization> = Vec::new();
        let mut inputs: Vec<String> = Vec::new();
        let mut output: Option<String> = None;
        let mut workgroup: Option<(u32, u32, u32)> = None;
        let mut includes: Vec<String> = Vec::new();
        let mut dispatch: Option<DispatchKind> = None;
        let mut class: Option<ClassKind> = None;

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            let ks = key.to_string();

            match ks.as_str() {
                "source" | "entry" | "body_fn" | "output" => {
                    input.parse::<Token![=]>()?;
                    let val: LitStr = input.parse()?;
                    match ks.as_str() {
                        "source" => source = Some(val.value()),
                        "entry" => entry = Some(val.value()),
                        "body_fn" => body_fn = Some(val.value()),
                        "output" => output = Some(val.value()),
                        _ => unreachable!(),
                    }
                }

                "includes" => {
                    let content;
                    syn::parenthesized!(content in input);
                    let paths: Punctuated<LitStr, Token![,]> =
                        content.parse_terminated(syn::parse::Parse::parse, Token![,])?;
                    includes = paths.into_iter().map(|p| p.value()).collect();
                }

                "specialize" => {
                    let content;
                    syn::parenthesized!(content in input);
                    let raw_types: Punctuated<Ident, Token![,]> =
                        Punctuated::parse_separated_nonempty(&content)?;

                    let id = raw_types
                        .iter()
                        .map(|t| t.to_string().to_lowercase().replace("codec", ""))
                        .collect::<Vec<_>>()
                        .join("_");

                    let fmts: Vec<Ident> = if content.peek(Token![=>]) {
                        content.parse::<Token![=>]>()?;
                        content
                            .parse_terminated(Ident::parse, Token![,])?
                            .into_iter()
                            .collect()
                    } else {
                        // Auto-derive formats from codec types
                        raw_types
                            .iter()
                            .flat_map(|t| {
                                codec_formats(&t.to_string())
                                    .into_iter()
                                    .map(|f| Ident::new(f, t.span()))
                                    .collect::<Vec<_>>()
                            })
                            .collect()
                    };

                    specs.push(Specialization {
                        id,
                        types: raw_types.into_iter().collect(),
                        formats: fmts,
                    });
                }

                "inputs" => {
                    let content;
                    syn::parenthesized!(content in input);
                    let names: Punctuated<Ident, Token![,]> =
                        content.parse_terminated(Ident::parse, Token![,])?;
                    inputs = names.into_iter().map(|i| i.to_string()).collect();
                }

                "workgroup" => {
                    let content;
                    syn::parenthesized!(content in input);
                    let x: LitInt = content.parse()?;
                    content.parse::<Token![,]>()?;
                    let y: LitInt = content.parse()?;
                    content.parse::<Token![,]>()?;
                    let z: LitInt = content.parse()?;
                    workgroup = Some((x.base10_parse()?, y.base10_parse()?, z.base10_parse()?));
                }

                "dispatch" => {
                    let content;
                    syn::parenthesized!(content in input);
                    match content.parse::<Ident>()?.to_string().as_str() {
                        "PerPixel" => dispatch = Some(DispatchKind::PerPixel),
                        o => {
                            return Err(syn::Error::new(
                                content.span(),
                                format!("unknown dispatch: {o}"),
                            ));
                        }
                    }
                }

                "class" => {
                    let content;
                    syn::parenthesized!(content in input);
                    match content.parse::<Ident>()?.to_string().as_str() {
                        "PerPixel" => class = Some(ClassKind::PerPixel),
                        o => {
                            return Err(syn::Error::new(
                                content.span(),
                                format!("unknown class: {o}"),
                            ));
                        }
                    }
                }

                other => return Err(syn::Error::new(key.span(), format!("unknown key: {other}"))),
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(KernelArgs {
            source: source.ok_or_else(|| input.error("missing `source`"))?,
            entry: entry.ok_or_else(|| input.error("missing `entry`"))?,
            body_fn: body_fn.ok_or_else(|| input.error("missing `body_fn`"))?,
            includes,
            specs,
            inputs,
            output: output.ok_or_else(|| input.error("missing `output`"))?,
            workgroup: workgroup.ok_or_else(|| input.error("missing `workgroup`"))?,
            dispatch: dispatch.unwrap_or(DispatchKind::PerPixel),
            class: class.unwrap_or(ClassKind::PerPixel),
        })
    }
}

// ── slangc compilation ──────────────────────────────────────────────────────

fn find_slangc() -> PathBuf {
    for c in &["slangc", "/home/keslarek/.local/bin/slangc"] {
        if Command::new(c)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok()
        {
            return PathBuf::from(*c);
        }
    }
    panic!("slangc not found");
}

fn compile_slang(
    slangc: &PathBuf,
    wrapper_content: &str,
    wrapper_name: &str,
    entry_name: &str,
    manifest_dir: &str,
    target_dir: &str,
    includes: &[String],
) {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap_or_else(|_| "/tmp".into()));
    let wrap_path = out.join(format!("{wrapper_name}.slang"));
    let spv_path = PathBuf::from(target_dir).join(format!("{wrapper_name}.spv"));

    fs::write(&wrap_path, wrapper_content).unwrap();

    let mut cmd = Command::new(slangc);
    cmd.arg(&wrap_path);
    cmd.arg("-I").arg(manifest_dir);
    for inc in includes {
        cmd.arg("-I").arg(format!("{manifest_dir}/{inc}"));
    }
    let status = cmd
        .arg("-entry")
        .arg(entry_name)
        .arg("-stage")
        .arg("compute")
        .arg("-target")
        .arg("spirv")
        .arg("-fvk-use-entrypoint-name")
        .arg("-fvk-use-scalar-layout")
        .arg("-o")
        .arg(&spv_path)
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => panic!("slangc failed for {wrapper_name}: exit {s}"),
        Err(e) => panic!("slangc error for {wrapper_name}: {e}"),
    }
}

// ── proc macro entry point ──────────────────────────────────────────────────

#[proc_macro_attribute]
pub fn kernel(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let args = match syn::parse::<KernelArgs>(attr) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error().into(),
    };

    let struct_ident = &input.ident;
    let name = args.entry.clone();
    let name_up = name.to_uppercase();
    let k_struct_name = format_ident!("{}Kernel", struct_ident);

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let target_dir = format!("{manifest_dir}/target/pixors-kernels");
    fs::create_dir_all(&target_dir).unwrap();

    let slangc = find_slangc();
    let (wg_x, wg_y, wg_z) = args.workgroup;

    // ── compile SPV for each specialization ──
    for s in &args.specs {
        let wrapper_name = format!("{}_{}", name, s.id);
        let entry_name = wrapper_name.clone();
        let type_args = s
            .types
            .iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
            .join(", ");

        let wrapper = format!(
            "import \"shaders/lib/codecs\";\n\
             import \"{source}\";\n\
             \n\
             [numthreads({wg_x}, {wg_y}, {wg_z})]\n\
             void {entry}(uint3 gid : SV_DispatchThreadID) {{\n\
                 {body_fn}<{type_args}>(gid);\n\
             }}\n",
            source = args.source,
            body_fn = args.body_fn,
            entry = &entry_name,
        );

        compile_slang(
            &slangc,
            &wrapper,
            &wrapper_name,
            &entry_name,
            &manifest_dir,
            &target_dir,
            &args.includes,
        );
    }

    // ── extract struct fields for params ──
    let fields: Vec<(Ident, syn::Type)> = match &input.data {
        syn::Data::Struct(ds) => ds
            .fields
            .iter()
            .map(|f| (f.ident.clone().unwrap(), f.ty.clone()))
            .collect(),
        _ => panic!("#[kernel] only supports structs"),
    };

    // ── SPV constants ──
    let spv_consts: Vec<_> = args.specs.iter().map(|s| {
        let cname = format_ident!("{}_SPV_{}", name_up, s.id.to_uppercase());
        let spv_file = format!("{}_{}.spv", name, s.id);
        quote! { pub const #cname: &[u8] = include_bytes!(concat!(#manifest_dir, "/target/pixors-kernels/", #spv_file)); }
    }).collect();

    // ── input / output declarations ──
    let input_decls: Vec<_> = args.inputs.iter().map(|n| {
        let n_str = n.as_str();
        quote! { ::pixors_engine::gpu::kernel::ResourceDeclaration { name: #n_str, element: ::pixors_engine::gpu::kernel::BindingElement::Image, access: ::pixors_engine::gpu::kernel::BindingAccess::Read } }
    }).collect();
    let out_str = args.output.as_str();
    let output_decl = quote! { ::pixors_engine::gpu::kernel::ResourceDeclaration { name: #out_str, element: ::pixors_engine::gpu::kernel::BindingElement::Image, access: ::pixors_engine::gpu::kernel::BindingAccess::Write } };
    let input_static = format_ident!("{}_INPUTS", name_up);
    let output_static = format_ident!("{}_OUTPUTS", name_up);

    // ── param declarations ──
    let param_decls: Vec<_> = fields.iter().map(|(fname, fty)| {
        let fname_str = fname.to_string();
        let kind = match quote!(#fty).to_string().as_str() {
            "u32" => quote! { ::pixors_engine::gpu::kernel::ParameterType::U32 },
            "i32" => quote! { ::pixors_engine::gpu::kernel::ParameterType::I32 },
            "f32" => quote! { ::pixors_engine::gpu::kernel::ParameterType::F32 },
            other => panic!("#[kernel] unsupported param type: {other}"),
        };
        quote! { ::pixors_engine::gpu::kernel::ParameterDeclaration { name: #fname_str, kind: #kind } }
    }).collect();
    let params_static = format_ident!("{}_PARAMS_DECL", name_up);

    // ── detect multi-param (color convert: src + dst) ──
    let is_multi = args.specs.iter().any(|s| s.id.contains('_'));

    // ── format map ──
    let format_entries: Vec<_> = if is_multi {
        args.specs.iter().flat_map(|s| {
            let cname = format_ident!("{}_SPV_{}", name_up, s.id.to_uppercase());
            let entry_name_str = format!("{}_{}", name, s.id);
            let src_fmts = codec_formats(&s.types[0].to_string());
            let dst_fmts = codec_formats(&s.types[1].to_string());
            let mut entries: Vec<proc_macro2::TokenStream> = Vec::new();
            for sf in &src_fmts {
                let sf_ident = format_ident!("{}", sf);
                for df in &dst_fmts {
                    let df_ident = format_ident!("{}", df);
                    let en = &entry_name_str;
                    let cn = &cname;
                    entries.push(quote! {
                        ((::pixors_engine::common::pixel::PixelFormat::#sf_ident, ::pixors_engine::common::pixel::PixelFormat::#df_ident), #en, & #cn)
                    });
                }
            }
            entries
        }).collect()
    } else {
        args.specs
            .iter()
            .flat_map(|s| {
                let cname = format_ident!("{}_SPV_{}", name_up, s.id.to_uppercase());
                let entry_name = format!("{}_{}", name, s.id);
                s.formats.iter().map(move |fmt| {
                let pf = format_ident!("{}", fmt.to_string());
                quote! { (::pixors_engine::common::pixel::PixelFormat::#pf, #entry_name, & #cname) }
            }).collect::<Vec<_>>()
            })
            .collect()
    };

    let fmt_map_static = format_ident!("{}_FORMAT_MAP", name_up);

    // ── dispatch / class ──
    let dispatch_shape = match args.dispatch {
        DispatchKind::PerPixel => quote! { ::pixors_engine::gpu::kernel::DispatchShape::PerPixel },
    };
    let kernel_class = match args.class {
        ClassKind::PerPixel => quote! { ::pixors_engine::gpu::kernel::KernelClass::PerPixel },
    };

    // ── emit ─────────────────────────────────────────────────────────────────
    let fmt_map_type = if is_multi {
        quote! { ((::pixors_engine::common::pixel::PixelFormat, ::pixors_engine::common::pixel::PixelFormat), &'static str, &'static [u8]) }
    } else {
        quote! { (::pixors_engine::common::pixel::PixelFormat, &'static str, &'static [u8]) }
    };

    let default_ctor = if is_multi {
        quote! {
            pub fn new(params: #struct_ident, src_fmt: ::pixors_engine::common::pixel::PixelFormat, dst_fmt: ::pixors_engine::common::pixel::PixelFormat) -> Self {
                let target = (src_fmt, dst_fmt);
                let (entry, spv) = #fmt_map_static.iter()
                    .find(|(k, _, _)| *k == target)
                    .map(|(_, e, s)| (*e, *s))
                    .unwrap_or_else(|| #fmt_map_static.first().map(|(_, e, s)| (*e, *s)).unwrap());
                Self { params, default_sig: ::pixors_engine::gpu::kernel::KernelSignature {
                    name: entry, entry,
                    inputs: #input_static, outputs: #output_static, params: #params_static,
                    workgroup: (#wg_x, #wg_y, #wg_z), dispatch: #dispatch_shape, class: #kernel_class, body: spv,
                } }
            }
        }
    } else {
        quote! {
            pub fn new(params: #struct_ident, fmt: ::pixors_engine::common::pixel::PixelFormat) -> Self {
                let (entry, spv) = #fmt_map_static.iter()
                    .find(|(f, _, _)| *f == fmt)
                    .or_else(|| #fmt_map_static.first())
                    .map(|(_, e, s)| (*e, *s))
                    .expect("FORMAT_MAP must not be empty");
                Self { params, default_sig: ::pixors_engine::gpu::kernel::KernelSignature {
                    name: entry, entry,
                    inputs: #input_static, outputs: #output_static, params: #params_static,
                    workgroup: (#wg_x, #wg_y, #wg_z), dispatch: #dispatch_shape, class: #kernel_class, body: spv,
                } }
            }
        }
    };

    let expanded = quote! {
        #input

        unsafe impl ::bytemuck::Pod for #struct_ident {}
        unsafe impl ::bytemuck::Zeroable for #struct_ident {}
        impl ::core::clone::Clone for #struct_ident { fn clone(&self) -> Self { *self } }
        impl ::core::marker::Copy for #struct_ident {}

        #(#spv_consts)*

        #[allow(non_upper_case_globals)]
        pub static #input_static: &[::pixors_engine::gpu::kernel::ResourceDeclaration] = &[
            #(#input_decls,)*
        ];

        #[allow(non_upper_case_globals)]
        pub static #output_static: &[::pixors_engine::gpu::kernel::ResourceDeclaration] = &[
            #output_decl,
        ];

        #[allow(non_upper_case_globals)]
        pub static #params_static: &[::pixors_engine::gpu::kernel::ParameterDeclaration] = &[
            #(#param_decls,)*
        ];

        #[allow(non_upper_case_globals)]
        pub static #fmt_map_static: &[#fmt_map_type] = &[
            #(#format_entries,)*
        ];

        pub struct #k_struct_name {
            pub params: #struct_ident,
            default_sig: ::pixors_engine::gpu::kernel::KernelSignature,
        }

        impl #k_struct_name {
            #default_ctor
        }

        impl ::pixors_engine::gpu::kernel::GpuKernel for #k_struct_name {
            fn signature(&self) -> &::pixors_engine::gpu::kernel::KernelSignature { &self.default_sig }

            fn write_params(&self, dst: &mut [u8]) {
                let bytes = bytemuck::bytes_of(&self.params);
                let n = bytes.len().min(dst.len());
                dst[..n].copy_from_slice(&bytes[..n]);
            }
        }
    };

    TokenStream::from(expanded)
}
