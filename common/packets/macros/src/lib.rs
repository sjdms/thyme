use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse::ParseStream,
    parse_macro_input, Ident, ItemStruct, LitInt, Result, Token,
};

struct PacketArgs {
    id: LitInt,
    direction: Direction,
}

enum Direction {
    Clientbound,
    Serverbound,
}

impl syn::parse::Parse for PacketArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut id = None;
        let mut direction = None;

        while !input.is_empty() {
            let key: Ident = input.parse()?;

            if key == "id" {
                input.parse::<Token![=]>()?;
                id = Some(input.parse()?);
            } else if key == "clientbound" {
                direction = Some(Direction::Clientbound);
            } else if key == "serverbound" {
                direction = Some(Direction::Serverbound);
            } else {
                return Err(syn::Error::new(
                    key.span(),
                    "expected `id`, `clientbound`, or `serverbound`",
                ));
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(Self {
            id: id.ok_or_else(|| syn::Error::new(input.span(), "missing `id`"))?,
            direction: direction
                .ok_or_else(|| syn::Error::new(input.span(), "missing direction"))?,
        })
    }
}

#[proc_macro_attribute]
pub fn packet(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as PacketArgs);
    let packet = parse_macro_input!(item as ItemStruct);

    let name = &packet.ident;
    let id = &args.id;

    let direction = match args.direction {
        Direction::Clientbound => quote! { ::packet_datatypes::Direction::Clientbound },
        Direction::Serverbound => quote! { ::packet_datatypes::Direction::Serverbound },
    };

    let mut decode_stmts = Vec::new();
    let mut encode_stmts = Vec::new();

    let fields_code = match &packet.fields {
        syn::Fields::Named(fields) => {
            for f in &fields.named {
                let ident = f.ident.as_ref().unwrap();
                let ty = &f.ty;
                decode_stmts.push(quote! { 
                    #ident: <#ty as ::packet_datatypes::PacketField>::decode(reader)? 
                });
                encode_stmts.push(quote! { 
                    ::packet_datatypes::PacketField::encode(&self.#ident, writer)?; 
                });
            }
            quote! { Self { #(#decode_stmts),* } }
        },
        syn::Fields::Unnamed(fields) => {
            for (i, f) in fields.unnamed.iter().enumerate() {
                let ty = &f.ty;
                let idx = syn::Index::from(i);
                decode_stmts.push(quote! { 
                    <#ty as ::packet_datatypes::PacketField>::decode(reader)? 
                });
                encode_stmts.push(quote! { 
                    ::packet_datatypes::PacketField::encode(&self.#idx, writer)?; 
                });
            }
            quote! { Self ( #(#decode_stmts),* ) }
        },
        syn::Fields::Unit => {
            quote! { Self }
        }
    };

    quote! {
        #packet

        impl ::packet_datatypes::PacketInfo for #name {
            const ID: u32 = #id;
            const DIRECTION: ::packet_datatypes::Direction = #direction;
        }

        impl ::packet_datatypes::Packet for #name {
            fn id(&self) -> u32 {
                <Self as ::packet_datatypes::PacketInfo>::ID
            }

            fn direction(&self) -> ::packet_datatypes::Direction {
                <Self as ::packet_datatypes::PacketInfo>::DIRECTION
            }

            fn decode(reader: &mut ::packet_datatypes::PacketReader<'_>) -> ::packet_datatypes::error::Result<Self> {
                Ok(#fields_code)
            }

            fn encode(&self, writer: &mut ::packet_datatypes::PacketWriter) -> ::packet_datatypes::error::Result<()> {
                #(#encode_stmts)*
                Ok(())
            }
        }
    }
    .into()
}

fn packet_field_fields(
    fields: &syn::Fields,
    decode_ctor: proc_macro2::TokenStream,
) -> (
    proc_macro2::TokenStream, // decode expression
    proc_macro2::TokenStream, // pattern to bind `self`'s fields for encoding
    proc_macro2::TokenStream, // encode statements
) {
    match fields {
        syn::Fields::Unit => {
            let decode_expr = quote! { #decode_ctor };
            let pattern = quote! { #decode_ctor };
            let encode_stmts = quote! {};
            (decode_expr, pattern, encode_stmts)
        }
        syn::Fields::Unnamed(fields) => {
            let field_names: Vec<_> = (0..fields.unnamed.len())
                .map(|i| syn::Ident::new(&format!("f{}", i), proc_macro2::Span::call_site()))
                .collect();

            let decodes: Vec<_> = fields.unnamed.iter().map(|f| {
                let ty = &f.ty;
                quote! { <#ty as ::packet_datatypes::PacketField>::decode(reader)? }
            }).collect();

            let encode_stmts: Vec<_> = field_names.iter().map(|f| {
                quote! { ::packet_datatypes::PacketField::encode(#f, writer)?; }
            }).collect();

            let decode_expr = quote! { #decode_ctor( #(#decodes),* ) };
            let pattern = quote! { #decode_ctor( #(#field_names),* ) };
            (decode_expr, pattern, quote! { #(#encode_stmts)* })
        }
        syn::Fields::Named(fields) => {
            let field_names: Vec<_> = fields
                .named
                .iter()
                .map(|f| f.ident.as_ref().unwrap().clone())
                .collect();

            let decodes: Vec<_> = fields.named.iter().map(|f| {
                let field_name = f.ident.as_ref().unwrap();
                let ty = &f.ty;
                quote! { #field_name: <#ty as ::packet_datatypes::PacketField>::decode(reader)? }
            }).collect();

            let encode_stmts: Vec<_> = field_names.iter().map(|f| {
                quote! { ::packet_datatypes::PacketField::encode(#f, writer)?; }
            }).collect();

            let decode_expr = quote! { #decode_ctor { #(#decodes),* } };
            let pattern = quote! { #decode_ctor { #(#field_names),* } };
            (decode_expr, pattern, quote! { #(#encode_stmts)* })
        }
    }
}

#[proc_macro_derive(PacketField)]
pub fn derive_packet_field(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as syn::DeriveInput);
    let name = &input.ident;

    match &input.data {
        syn::Data::Enum(data) => {
            let mut encode_arms = Vec::new();
            let mut decode_arms = Vec::new();

            for (i, variant) in data.variants.iter().enumerate() {
                let variant_name = &variant.ident;
                let tag = i as u8; // We use a u8 for the discriminant
                let ctor = quote! { #name::#variant_name };

                let (decode_expr, pattern, encode_stmts) =
                    packet_field_fields(&variant.fields, ctor);

                encode_arms.push(quote! {
                    #pattern => {
                        writer.write_u8(#tag);
                        #encode_stmts
                    }
                });
                decode_arms.push(quote! {
                    #tag => Ok(#decode_expr),
                });
            }

            quote! {
                impl ::packet_datatypes::PacketField for #name {
                    fn decode(reader: &mut ::packet_datatypes::PacketReader<'_>) -> ::packet_datatypes::error::Result<Self> {
                        let tag = reader.read_u8()?;
                        match tag {
                            #(#decode_arms)*
                            _ => Err(::std::io::Error::new(
                                ::std::io::ErrorKind::InvalidData,
                                format!("invalid enum tag: {}", tag),
                            ).into()),
                        }
                    }

                    fn encode(&self, writer: &mut ::packet_datatypes::PacketWriter) -> ::packet_datatypes::error::Result<()> {
                        match self {
                            #(#encode_arms)*
                        }
                        Ok(())
                    }
                }
            }
            .into()
        }
        syn::Data::Struct(data) => {
            let ctor = quote! { Self };
            let (decode_expr, pattern, encode_stmts) = packet_field_fields(&data.fields, ctor);

            quote! {
                impl ::packet_datatypes::PacketField for #name {
                    fn decode(reader: &mut ::packet_datatypes::PacketReader<'_>) -> ::packet_datatypes::error::Result<Self> {
                        Ok(#decode_expr)
                    }

                    fn encode(&self, writer: &mut ::packet_datatypes::PacketWriter) -> ::packet_datatypes::error::Result<()> {
                        let #pattern = self;
                        #encode_stmts
                        Ok(())
                    }
                }
            }
            .into()
        }
        syn::Data::Union(_) => {
            syn::Error::new_spanned(name, "PacketField cannot be derived for unions")
                .to_compile_error()
                .into()
        }
    }
}