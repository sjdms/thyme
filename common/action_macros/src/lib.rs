use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, LitStr};

#[proc_macro_derive(ThymeActions, attributes(action_description))]
pub fn derive_thyme_actions(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;

    // Ensure this is only derived on Enums
    let Data::Enum(data_enum) = &ast.data else {
        panic!("ThymeActions can only be derived on enums");
    };

    let mut action_descriptions = String::new();
    let mut llm_instructions = String::from(
        "You are an AI assistant. You can perform actions by outputting JSON.\n\
        The JSON must match the schemas below. Always include the `action` tag.\n\n"
    );

    // Iterate through all variants (e.g., AddClinicalNote, AddDiagnosis)
    for variant in &data_enum.variants {
        let variant_name = &variant.ident;
        let mut description = String::new();

        // 1. Extract the #[action_description("...")] attribute
        for attr in &variant.attrs {
            if attr.path().is_ident("action_description") {
                if let Ok(lit) = attr.parse_args::<LitStr>() {
                    description = lit.value();
                }
            }
        }

        action_descriptions.push_str(&format!("- Action: **{}**\n  Description: {}\n", variant_name, description));
        llm_instructions.push_str(&format!("### Action: {}\n{}\n```json\n{{\n  \"action\": \"{}\"", variant_name, description, variant_name));

        // 2. Extract fields and their types
        if let Fields::Named(fields) = &variant.fields {
            for field in &fields.named {
                let field_name = field.ident.as_ref().unwrap();
                
                // Convert the type tokens to a string (e.g., "Option < String >") and clean up spaces.
                // NOTE: must bind `&field.ty` to its own variable before interpolating —
                // `quote!(#field.ty)` would interpolate the whole `field` (printing the
                // entire `name: Type` pair) and then append a literal `.ty`, since `quote!`'s
                // `#ident` interpolation only ever consumes the identifier immediately after `#`.
                let ty = &field.ty;
                let field_type = quote!(#ty).to_string().replace(" ", "");

                action_descriptions.push_str(&format!("  - `{}`: `{}`\n", field_name, field_type));
                llm_instructions.push_str(&format!(",\n  \"{}\": \"<{}>\"", field_name, field_type));
            }
        }
        
        action_descriptions.push_str("\n");
        llm_instructions.push_str("\n}\n```\n\n");
    }

    // 3. Generate the implementation block
    let expanded = quote! {
        impl #name {
            /// Returns a human-readable list of all available actions and their arguments.
            pub fn available_actions() -> &'static str {
                #action_descriptions
            }

            /// Returns a strictly formatted string of instructions for an LLM to generate valid JSON.
            pub fn llm_instructions() -> &'static str {
                #llm_instructions
            }
        }
    };

    TokenStream::from(expanded)
}