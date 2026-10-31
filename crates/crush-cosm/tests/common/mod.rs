//! The design doc's worked examples (docs/design/object-model.md, "Worked
//! examples"), built in Rust. `tests/fixtures/*.cosm.json` are their JSON.
#![allow(dead_code)]

use crush_cosm::*;

fn sp(line: u32, col: u32) -> Option<Span> {
    Some(Span::new(line, col).with_file("example"))
}

/// ```rust
/// #[derive(Clone, PartialEq)]
/// struct Point { x: i64, y: i64 }
/// impl Point {
///     fn new(x: i64, y: i64) -> Self { Point { x, y } }
///     fn norm2(&self) -> i64 { self.x * self.x + self.y * self.y }
///     fn shift(&mut self, d: i64) { self.x += d; }
/// }
/// impl std::ops::Add for Point { type Output = Point; fn add(self, o: Point) -> Point { /* … */ } }
/// trait Shape { fn area(&self) -> f64; }
/// impl Shape for Point { fn area(&self) -> f64 { 0.0 } }
/// ```
pub fn rust_point() -> CosmModule {
    let mut point = KindDecl {
        name: "Point".into(),
        home_lang: "rust".into(),
        span: sp(2, 1),
        storage: Storage::Sealed {
            fields: vec![
                FieldDecl::new("x", CosmType::Int),
                FieldDecl::new("y", CosmType::Int),
            ],
        },
        semantics: Semantics::Value(ValueMode::Move),
        parents: vec![],
        resolution: Resolution::None,
        receiver: Receiver::ExplicitValue,
        construct: Construct::Literal,
        members: vec![
            MemberDecl::static_fn("new", "Point::new").with_span(Span::new(5, 5)),
            MemberDecl::method_with("norm2", "Point::norm2", Receiver::ExplicitRef)
                .with_span(Span::new(6, 5)),
            MemberDecl::method_with("shift", "Point::shift", Receiver::ExplicitRefMut)
                .with_span(Span::new(7, 5)),
        ],
        hooks: Hooks {
            eq: Some(HookImpl::Derived),
            clone: Some(HookImpl::Derived),
            ..Hooks::default()
        },
        conforms: vec![],
        equality: Equality::Structural,
    };
    point
        .hooks
        .op
        .insert(Op::Add, HookImpl::Fn("<Point as Add>::add".into()));
    let mut shape_impl = Conformance::new("Shape", Satisfaction::Nominal);
    shape_impl.span = sp(11, 1);
    shape_impl.members.push(MemberDecl::method_with(
        "area",
        "<Point as Shape>::area",
        Receiver::ExplicitRef,
    ));
    point.conforms.push(shape_impl);

    let shape = InterfaceDecl {
        name: "Shape".into(),
        home_lang: "rust".into(),
        span: sp(10, 1),
        members: vec![InterfaceMember::method(
            "area",
            Receiver::ExplicitRef,
            CosmType::Float,
        )],
        satisfaction: Satisfaction::Nominal,
    };
    CosmModule {
        kinds: vec![point],
        interfaces: vec![shape],
    }
}

/// Python's root: what a Python program's module is validated against.
pub fn python_prelude() -> CosmModule {
    CosmModule {
        kinds: vec![KindDecl {
            name: "object".into(),
            home_lang: "python".into(),
            span: None,
            storage: Storage::OpenDict,
            semantics: Semantics::Reference,
            parents: vec![],
            resolution: Resolution::C3,
            receiver: Receiver::BoundSelf,
            construct: Construct::PyNewInit {
                new_fn: None,
                init_fn: None,
            },
            members: vec![],
            hooks: Hooks::default(),
            conforms: vec![],
            equality: Equality::Identity,
        }],
        interfaces: vec![],
    }
}

/// ```python
/// class Animal:
///     kind = "animal"
///     def __init__(self, name): self.name = name
///     def speak(self): return f"{self.name} makes a sound"
///     def __repr__(self): return f"Animal({self.name!r})"
///
/// class Dog(Animal):
///     def speak(self): return super().speak() + " (woof)"
///     def __eq__(self, other): return isinstance(other, Dog) and self.name == other.name
/// ```
pub fn python_animals() -> CosmModule {
    let python_class = |name: &str, parent: &str, line: u32| KindDecl {
        name: name.into(),
        home_lang: "python".into(),
        span: sp(line, 1),
        storage: Storage::OpenDict,
        semantics: Semantics::Reference,
        parents: vec![parent.into()],
        resolution: Resolution::C3,
        receiver: Receiver::BoundSelf,
        construct: Construct::PyNewInit {
            new_fn: None,
            init_fn: None,
        },
        members: vec![],
        hooks: Hooks::default(),
        conforms: vec![],
        equality: Equality::Identity,
    };
    let mut animal = python_class("Animal", "object", 1);
    animal.construct = Construct::PyNewInit {
        new_fn: None,
        init_fn: Some("Animal.__init__".into()),
    };
    animal.members = vec![
        MemberDecl::static_const("kind", Const::Str("animal".into())).with_span(Span::new(2, 5)),
        MemberDecl::method("speak", "Animal.speak").with_span(Span::new(4, 5)),
    ];
    animal.hooks.repr = Some(HookImpl::Fn("Animal.__repr__".into()));

    // Dog inherits __init__: `init_fn: None` looks it up through the MRO.
    let mut dog = python_class("Dog", "Animal", 7);
    dog.members = vec![MemberDecl::method("speak", "Dog.speak").with_span(Span::new(8, 5))];
    dog.hooks.eq = Some(HookImpl::Fn("Dog.__eq__".into()));
    // Defining __eq__ without __hash__ makes instances unhashable.
    dog.hooks.hash = Some(HookImpl::Disabled);

    CosmModule {
        kinds: vec![animal, dog],
        interfaces: vec![],
    }
}

/// ```crush
/// struct Point { x: int, y: int }
/// impl Point {
///     fn new(x, y) { return Point { x: x, y: y }; }
///     fn norm2(self) { return self.x * self.x + self.y * self.y; }
/// }
/// trait Shape { fn area(self); fn describe(self) { return "a shape"; } }   // default method
/// impl Shape for Point { fn area(self) { return 0; } }
/// ```
pub fn crush_point() -> CosmModule {
    let mut point = KindDecl::crush("Point");
    point.span = sp(1, 1);
    point.storage = Storage::Sealed {
        fields: vec![
            FieldDecl::new("x", CosmType::Int),
            FieldDecl::new("y", CosmType::Int),
        ],
    };
    point.members = vec![
        MemberDecl::static_fn("new", "Point::new").with_span(Span::new(3, 5)),
        MemberDecl::method("norm2", "Point::norm2").with_span(Span::new(4, 5)),
    ];
    let mut shape_impl = Conformance::new("Shape", Satisfaction::Nominal);
    shape_impl.span = sp(7, 1);
    shape_impl
        .members
        .push(MemberDecl::method("area", "<Point as Shape>::area"));
    point.conforms.push(shape_impl);

    let shape = InterfaceDecl {
        name: "Shape".into(),
        home_lang: "crush".into(),
        span: sp(6, 1),
        members: vec![
            InterfaceMember::method("area", Receiver::ExplicitRef, CosmType::Any),
            InterfaceMember::method("describe", Receiver::ExplicitRef, CosmType::Any)
                .with_default("Shape::describe"),
        ],
        satisfaction: Satisfaction::Nominal,
    };
    CosmModule {
        kinds: vec![point],
        interfaces: vec![shape],
    }
}
