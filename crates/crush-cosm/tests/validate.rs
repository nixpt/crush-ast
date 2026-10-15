//! One test per diagnostic: start from a valid worked example, break one
//! thing, and check the code, subject, span and message.

mod common;

use crush_cosm::*;

fn codes(diags: &[Diagnostic]) -> Vec<DiagnosticCode> {
    diags.iter().map(|d| d.code).collect()
}

fn only(diags: Vec<Diagnostic>, code: DiagnosticCode) -> Diagnostic {
    assert_eq!(codes(&diags), vec![code], "{diags:#?}");
    diags.into_iter().next().unwrap()
}

fn plain(name: &str, resolution: Resolution, parents: &[&str]) -> KindDecl {
    let mut k = KindDecl::crush(name);
    k.resolution = resolution;
    k.parents = parents.iter().map(|p| KindRef::from(*p)).collect();
    k.span = Some(Span::new(1, 1));
    k
}

#[test]
fn duplicate_decl() {
    let mut m = common::crush_point();
    m.interfaces[0].name = "Point".into(); // collides with the kind
    m.kinds[0].conforms.clear();
    let d = only(validate(&m), DiagnosticCode::DuplicateDecl);
    assert_eq!(d.subject, "Point");
    assert!(d.message.contains("interface `Point`"), "{}", d.message);
}

#[test]
fn unknown_parent_and_parent_is_interface() {
    let mut m = common::crush_point();
    m.kinds[0].parents = vec!["Nope".into()];
    let d = only(validate(&m), DiagnosticCode::UnknownParent);
    assert_eq!(d.message, "unknown parent kind `Nope`");
    assert_eq!(d.span, Some(Span::new(1, 1).with_file("example")));

    m.kinds[0].parents = vec!["Shape".into()];
    only(validate(&m), DiagnosticCode::ParentIsInterface);
}

#[test]
fn unknown_interface_and_conforms_to_kind() {
    let mut m = common::crush_point();
    m.kinds[0].conforms[0].interface = "Drawable".into();
    let d = only(validate(&m), DiagnosticCode::UnknownInterface);
    assert_eq!(d.subject, "Point: Drawable");
    assert_eq!(
        d.span,
        Some(Span::new(7, 1).with_file("example")),
        "conformance span"
    );

    m.kinds[0].conforms[0].interface = "Point".into();
    only(validate(&m), DiagnosticCode::ConformsToKind);
}

#[test]
fn duplicate_conformance() {
    let mut m = common::crush_point();
    let c = m.kinds[0].conforms[0].clone();
    m.kinds[0].conforms.push(c);
    only(validate(&m), DiagnosticCode::DuplicateConformance);
}

#[test]
fn duplicate_field() {
    let mut m = common::crush_point();
    if let Storage::Sealed { fields } = &mut m.kinds[0].storage {
        fields.push(FieldDecl::new("x", CosmType::Float));
    }
    let d = only(validate(&m), DiagnosticCode::DuplicateField);
    assert_eq!(d.subject, "Point.x");
}

#[test]
fn duplicate_member_in_kind_conformance_and_interface() {
    let mut m = common::rust_point();
    m.kinds[0]
        .members
        .push(MemberDecl::method("norm2", "Point::norm2_again").with_span(Span::new(9, 5)));
    let d = only(validate(&m), DiagnosticCode::DuplicateMember);
    assert_eq!(d.subject, "Point.norm2");
    assert_eq!(d.span, Some(Span::new(9, 5)), "the member's own span");

    let mut m = common::rust_point();
    let area = m.kinds[0].conforms[0].members[0].clone();
    m.kinds[0].conforms[0].members.push(area);
    let d = only(validate(&m), DiagnosticCode::DuplicateMember);
    assert_eq!(d.subject, "Point: Shape.area");

    let mut m = common::rust_point();
    let area = m.interfaces[0].members[0].clone();
    m.interfaces[0].members.push(area);
    let d = only(validate(&m), DiagnosticCode::DuplicateMember);
    assert_eq!(d.subject, "Shape.area");
}

#[test]
fn sealed_field_shadowed_by_method() {
    let mut m = common::rust_point();
    m.kinds[0].members.push(
        MemberDecl::method_with("x", "Point::x", Receiver::ExplicitRef).with_span(Span::new(8, 5)),
    );
    let d = only(validate(&m), DiagnosticCode::FieldShadowedByMember);
    assert_eq!(d.subject, "Point.x");
    assert_eq!(
        d.message,
        "method `x` shadows the sealed field of the same name"
    );
    assert_eq!(d.span, Some(Span::new(8, 5)));

    // A `field` member naming the sealed field is fine; an open kind has no sealed fields.
    let mut m = common::rust_point();
    m.kinds[0].members.push(MemberDecl::field("x"));
    assert_eq!(validate(&m), vec![]);
}

#[test]
fn undeclared_field_and_empty_accessor() {
    let mut m = common::crush_point();
    m.kinds[0].members.push(MemberDecl::field("z"));
    only(validate(&m), DiagnosticCode::UndeclaredField);

    let mut m = common::crush_point();
    m.kinds[0]
        .members
        .push(MemberDecl::accessor("len", None, None));
    only(validate(&m), DiagnosticCode::EmptyAccessor);
}

#[test]
fn value_copy_or_move_with_open_dict_is_rejected() {
    for mode in [ValueMode::Copy, ValueMode::Move] {
        for storage in [
            Storage::OpenDict,
            Storage::OpenDictWithSlots { slots: vec![] },
        ] {
            let mut m = common::crush_point();
            m.kinds[0].storage = storage.clone();
            m.kinds[0].semantics = Semantics::Value(mode);
            let d = only(validate(&m), DiagnosticCode::ValueSemanticsWithOpenStorage);
            assert_eq!(
                d.message,
                format!(
                    "`value({})` semantics need a fixed layout, but storage is `{}`",
                    mode.as_str(),
                    storage.strategy_name()
                )
            );
        }
    }
    // clone_explicit is not named by the rule; reference + open_dict is Python.
    let mut m = common::crush_point();
    m.kinds[0].storage = Storage::OpenDict;
    m.kinds[0].semantics = Semantics::Value(ValueMode::CloneExplicit);
    assert_eq!(validate(&m), vec![]);
}

#[test]
fn nominal_conformance_must_provide_every_member() {
    let mut m = common::rust_point();
    m.kinds[0].conforms[0].members.clear();
    let d = only(validate(&m), DiagnosticCode::MissingInterfaceMember);
    assert_eq!(d.subject, "Point: Shape");
    assert_eq!(d.message, "missing member(s) of interface `Shape`: `area`");
    assert_eq!(d.span, Some(Span::new(11, 1).with_file("example")));

    // A default method needs no impl (Crush `describe`); removing the default does.
    let mut m = common::crush_point();
    m.interfaces[0].members[1].default = None;
    let d = only(validate(&m), DiagnosticCode::MissingInterfaceMember);
    assert!(d.message.ends_with("`describe`"), "{}", d.message);
}

#[test]
fn conformance_satisfied_by_an_inherited_member() {
    // Python-style: an ABC's method provided by a base class, found via the MRO.
    let mut base = plain("Base", Resolution::C3, &[]);
    base.members.push(MemberDecl::method("area", "Base.area"));
    let mut child = plain("Child", Resolution::C3, &["Base"]);
    child
        .conforms
        .push(Conformance::new("Shape", Satisfaction::Nominal));
    let shape = InterfaceDecl {
        name: "Shape".into(),
        home_lang: "python".into(),
        span: None,
        members: vec![InterfaceMember::method(
            "area",
            Receiver::BoundSelf,
            CosmType::Any,
        )],
        satisfaction: Satisfaction::Nominal,
    };
    let mut m = CosmModule {
        kinds: vec![base, child],
        interfaces: vec![shape],
    };
    assert_eq!(validate(&m), vec![]);
    m.kinds[0].members.clear();
    only(validate(&m), DiagnosticCode::MissingInterfaceMember);
}

#[test]
fn impl_member_not_in_interface() {
    let mut m = common::rust_point();
    m.kinds[0].conforms[0].members.push(MemberDecl::method(
        "perimeter",
        "<Point as Shape>::perimeter",
    ));
    let d = only(validate(&m), DiagnosticCode::UnknownInterfaceMember);
    assert_eq!(d.subject, "Point: Shape.perimeter");
}

#[test]
fn satisfaction_mismatch() {
    let mut m = common::rust_point();
    m.kinds[0].conforms[0].check = Satisfaction::Structural;
    only(validate(&m), DiagnosticCode::SatisfactionMismatch);
}

#[test]
fn parent_arity_per_strategy() {
    let a = plain("A", Resolution::None, &[]);
    let b = plain("B", Resolution::None, &[]);
    for (resolution, parents, ok) in [
        (Resolution::None, vec!["A"], false),
        (Resolution::Single, vec!["A"], true),
        (Resolution::Single, vec!["A", "B"], false),
        (Resolution::PrototypeChain, vec!["A"], true),
        (Resolution::PrototypeChain, vec!["A", "B"], false),
        (Resolution::C3, vec!["A", "B"], true),
    ] {
        let k = plain("K", resolution, &parents);
        let m = CosmModule {
            kinds: vec![a.clone(), b.clone(), k],
            interfaces: vec![],
        };
        let diags = validate(&m);
        if ok {
            assert_eq!(diags, vec![], "{resolution:?} {parents:?}");
        } else {
            let d = only(diags, DiagnosticCode::ParentArity);
            assert_eq!(d.subject, "K");
        }
    }
}

#[test]
fn single_and_prototype_chains_are_linear() {
    let m = CosmModule {
        kinds: vec![
            plain("Object", Resolution::PrototypeChain, &[]),
            plain("Animal", Resolution::PrototypeChain, &["Object"]),
            plain("Dog", Resolution::PrototypeChain, &["Animal"]),
        ],
        interfaces: vec![],
    };
    assert_eq!(
        m.resolution_order("Dog").unwrap(),
        ["Dog", "Animal", "Object"].map(KindRef::from).to_vec()
    );
    assert_eq!(validate(&m), vec![]);
}

#[test]
fn inheritance_cycle_reported_once() {
    let m = CosmModule {
        kinds: vec![
            plain("A", Resolution::C3, &["C"]),
            plain("B", Resolution::C3, &["A"]),
            plain("C", Resolution::Single, &["B"]),
        ],
        interfaces: vec![],
    };
    let d = only(validate(&m), DiagnosticCode::InheritanceCycle);
    assert_eq!(d.subject, "A");
    assert_eq!(d.message, "inheritance cycle: A -> C -> B -> A");
    let err = m.c3_mro("B").unwrap_err();
    assert!(matches!(err, MroError::Cycle { .. }), "{err:?}");

    let selfish = CosmModule {
        kinds: vec![plain("S", Resolution::C3, &["S"])],
        interfaces: vec![],
    };
    let d = only(validate(&selfish), DiagnosticCode::InheritanceCycle);
    assert_eq!(d.message, "inheritance cycle: S -> S");
}

#[test]
fn embedding_promotion_is_valid_but_not_linear() {
    let inner = plain("Inner", Resolution::None, &[]);
    let outer = plain("Outer", Resolution::EmbeddingPromotion, &["Inner"]);
    let m = CosmModule {
        kinds: vec![inner.clone(), outer.clone()],
        interfaces: vec![],
    };
    assert_eq!(validate(&m), vec![]);
    assert!(matches!(
        m.resolution_order("Outer"),
        Err(MroError::NotLinear { .. })
    ));

    // ...so it can't be a c3/single parent.
    let child = plain("Child", Resolution::C3, &["Outer"]);
    let m = CosmModule {
        kinds: vec![inner, outer, child],
        interfaces: vec![],
    };
    let d = only(validate(&m), DiagnosticCode::IncompatibleParent);
    assert_eq!(d.subject, "Child");
}

#[test]
fn c3_on_a_kind_whose_own_resolution_is_not_c3() {
    // c3_mro applies C3 at the kind itself; resolution_order follows the strategy.
    let m = CosmModule {
        kinds: vec![
            plain("A", Resolution::None, &[]),
            plain("B", Resolution::Single, &["A"]),
        ],
        interfaces: vec![],
    };
    assert_eq!(
        m.c3_mro("B").unwrap(),
        ["B", "A"].map(KindRef::from).to_vec()
    );
    assert_eq!(m.c3_mro("A").unwrap(), vec![KindRef::from("A")]);
    assert!(matches!(m.c3_mro("Z"), Err(MroError::UnknownKind { .. })));
}

#[test]
fn diagnostics_display_with_span_and_code() {
    let mut m = common::rust_point();
    m.kinds[0].parents = vec!["Nope".into()];
    let d = only(validate(&m), DiagnosticCode::UnknownParent);
    assert_eq!(
        d.to_string(),
        "example:2:1: error[cosm::unknown_parent]: Point: unknown parent kind `Nope`"
    );
    // Diagnostics serialize for tools (LSP, polydex).
    let v = serde_json::to_value(&d).unwrap();
    assert_eq!(v["code"], "unknown_parent");
}
