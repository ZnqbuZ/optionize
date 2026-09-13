use super::*;

fn errors(input: TokenStream) -> Vec<String> {
    parse(Crate::default(), input)
        .expect_err("invalid attributes must be rejected")
        .flatten()
        .into_iter()
        .map(|error| error.to_string())
        .collect()
}

#[test]
fn attributes_report_errors_on_fields_after_an_invalid_attribute() {
    let messages = errors(quote! {
        struct Config {
            #[optionize(skip, flatten)]
            first: u32,
            #[optionize(name = "{}")]
            r#type: u32,
        }
    });
    assert_eq!(messages.len(), 2, "{messages:?}");
    assert!(messages[0].contains("`skip` attribute cannot be combined with other attributes"));
    assert!(messages[1].contains("expected identifier") && messages[1].contains("`type`"));
}

#[test]
fn attributes_reject_conflicting_mapping_options() {
    let cases = [
        (
            quote!(object = Patch, subject = Config),
            "`subject` and `object` cannot be combined",
        ),
        (
            quote!(object = Patch, name = "{}Patch"),
            "`name` cannot be used when `object` or `subject` is specified",
        ),
        (
            quote!(subject = Config, name = "{}Patch"),
            "`name` cannot be used when `object` or `subject` is specified",
        ),
        (
            quote!(object = Patch, attrs()),
            "`attrs` cannot be used when `object` or `subject` is specified",
        ),
        (
            quote!(subject = Config, attrs()),
            "`attrs` cannot be used when `object` or `subject` is specified",
        ),
        (
            quote!(object = Patch, partial(marked)),
            "`marked` cannot be used when `object` is specified",
        ),
    ];
    for (options, expected) in cases {
        let messages = errors(quote! {
            #[optionize(#options)]
            struct Local { value: u32 }
        });
        assert_eq!(messages.len(), 1, "{options}: {messages:?}");
        assert!(messages[0].contains(expected), "{options}: {messages:?}");
    }
}

#[test]
fn attributes_report_all_independent_mapping_conflicts() {
    let messages = errors(quote! {
        #[optionize(object = Patch, subject = Config, name = "{}Patch", attrs(), partial(marked))]
        struct Local { value: u32 }
    });
    assert_eq!(messages.len(), 4, "{messages:?}");
    for expected in [
        "`subject` and `object` cannot be combined",
        "`name` cannot be used",
        "`attrs` cannot be used",
        "`marked` cannot be used",
    ] {
        assert!(
            messages.iter().any(|message| message.contains(expected)),
            "{messages:?}"
        );
    }
}

#[test]
fn skip_rejects_combinations_with_other_field_options() {
    for option in [
        quote!(flatten),
        quote!(nest = ChildPatch),
        quote!(name = "renamed"),
        quote!(attrs()),
    ] {
        let messages = errors(quote! {
            #[optionize(partial)]
            struct Config {
                #[optionize(skip, #option)]
                value: u32,
            }
        });
        assert_eq!(messages.len(), 1, "{option}: {messages:?}");
        assert!(
            messages[0].contains("`skip` attribute cannot be combined with other attributes"),
            "{option}: {messages:?}"
        );
    }
}

#[test]
fn attributes_reject_options_in_incompatible_struct_shapes() {
    let cases = [
        (
            quote!(
                struct Config {
                    #[optionize(skip)]
                    value: u32,
                }
            ),
            "`skip` attribute is only allowed when `partial` is specified",
        ),
        (
            quote!(
                struct Config(#[optionize(name = "value")] u32);
            ),
            "`name` attribute cannot be used on unnamed fields",
        ),
        (
            quote!(
                #[optionize(partial(marked(name = marker)))]
                struct Config(u32);
            ),
            "`name` attribute cannot be used on unnamed structs",
        ),
    ];
    for (input, expected) in cases {
        let messages = errors(input.clone());
        assert_eq!(messages.len(), 1, "{input}: {messages:?}");
        assert!(messages[0].contains(expected), "{input}: {messages:?}");
    }
}

#[test]
fn optionized_rejects_enums_and_unions() {
    for input in [
        quote!(
            enum Config {
                Value,
            }
        ),
        quote!(union Config { value: u32 }),
    ] {
        assert_eq!(errors(input), ["Optionize can only be derived for structs"]);
    }
}

#[test]
fn type_arguments_reject_non_paths_and_malformed_strings() {
    for value in [
        quote!(123),
        quote!(false),
        quote!((first, second)),
        quote!(factory()),
        quote!(&Config),
        quote!("model::"),
        quote!("model::Config<"),
    ] {
        for input in [
            quote!(
                #[optionize(object = #value)]
                struct Config {
                    value: u32,
                }
            ),
            quote!(
                #[optionize(subject = #value)]
                struct Patch {
                    value: Option<u32>,
                }
            ),
            quote!(
                struct Config {
                    #[optionize(nest = #value)]
                    value: Child,
                }
            ),
        ] {
            assert!(
                parse(Crate::default(), input.clone()).is_err(),
                "accepted invalid path: {input}"
            );
        }
    }
}

#[test]
fn attributes_reject_unknown_options_and_literal_attribute_entries() {
    for input in [
        quote!(
            #[optionize(unknown)]
            struct Config;
        ),
        quote!(
            struct Config {
                #[optionize(unknown)]
                value: u32,
            }
        ),
        quote!(
            #[optionize(attrs("invalid"))]
            struct Config;
        ),
    ] {
        assert_eq!(errors(input).len(), 1);
    }
    let input = quote!(
        struct Config;
    );
    let error =
        proc(quote!(unknown), &input).expect_err("unknown macro arguments must be rejected");
    assert!(error.to_string().contains("Unknown field"));
}

#[test]
fn attributes_accumulate_replacements_and_clear_inherited_field_attributes() {
    let output = parse(
        Crate::default(),
        quote! {
            #[optionize(attrs(derive(Clone)), attrs(doc = "generated"))]
            #[derive(Debug)]
            struct Config {
                #[doc = "inherited"]
                inherited: u32,
                #[doc = "removed"]
                #[optionize(attrs())]
                cleared: u32,
                #[doc = "original"]
                #[optionize(attrs(doc = "first"), attrs(doc = "second"))]
                replaced: u32,
            }
        },
    )
    .unwrap();
    let output = parse2::<syn::File>(output).unwrap();
    let object = output
        .items
        .iter()
        .find_map(|item| match item {
            syn::Item::Struct(item) if item.ident == "ConfigOptional" => Some(item),
            _ => None,
        })
        .unwrap();
    let attrs = &object.attrs;
    assert_eq!(
        quote!(#(#attrs)*).to_string(),
        quote!(#[derive(Clone)] #[doc = "generated"]).to_string()
    );
    let mut fields = object.fields.iter();
    let attrs = &fields.next().unwrap().attrs;
    assert_eq!(
        quote!(#(#attrs)*).to_string(),
        quote!(#[doc = "inherited"]).to_string()
    );
    assert!(fields.next().unwrap().attrs.is_empty());
    let attrs = &fields.next().unwrap().attrs;
    assert_eq!(
        quote!(#(#attrs)*).to_string(),
        quote!(#[doc = "first"] #[doc = "second"]).to_string()
    );
}
