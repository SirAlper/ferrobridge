//! Bağlanabilirlik kuralları ve adlandırma için tablo testleri.

use ferrobridge_ir::{
    Class, Constructor, Destructor, Enum, EnumVariant, Function, Item, Method, Module, Param,
    QualType, SourceLocation, Type, Unsupported,
};

use super::*;

/// Açıklamasız analiz (M2 davranışı).
fn check(module: &Module) -> BridgePlan {
    super::check(module, &Annotations::default()).unwrap()
}

/// `ferrobridge.toml` metniyle analiz.
fn check_with(module: &Module, toml: &str) -> Result<BridgePlan, Vec<AnnotationIssue>> {
    let annotations = Annotations::from_toml_str(toml, "/yerel/yol/ferrobridge.toml").unwrap();
    super::check(module, &annotations)
}

// ---- IR kurucuları -------------------------------------------------------------------------

fn loc() -> SourceLocation {
    SourceLocation {
        file: "input.h".to_owned(),
        line: 1,
        column: 1,
    }
}

fn q(ty: Type) -> QualType {
    QualType {
        is_const: false,
        ty,
    }
}

fn konst(ty: Type) -> QualType {
    QualType { is_const: true, ty }
}

fn int(spelling: &str, builtin: &str, size: u64, signed: bool, fixed: bool) -> Type {
    Type::Integer {
        spelling: spelling.to_owned(),
        builtin: builtin.to_owned(),
        size_bytes: size,
        signed,
        fixed_width: fixed,
    }
}

fn i32_ty() -> Type {
    int("int", "int", 4, true, false)
}

fn f64_ty() -> Type {
    Type::Float {
        spelling: "double".to_owned(),
        size_bytes: 8,
    }
}

fn record(name: &str) -> Type {
    Type::Record {
        name: name.to_owned(),
        namespace: vec!["geo".to_owned()],
    }
}

fn lref(inner: QualType) -> Type {
    Type::LValueRef {
        referent: Box::new(inner),
    }
}

fn param(name: &str, ty: QualType) -> Param {
    Param {
        name: Some(name.to_owned()),
        ty,
    }
}

fn function(ns: &[&str], name: &str, params: Vec<Param>, ret: QualType) -> Item {
    Item::Function(Function {
        name: name.to_owned(),
        namespace: ns.iter().map(|s| (*s).to_owned()).collect(),
        params,
        return_type: ret,
        noexcept: false,
        location: loc(),
    })
}

fn method(name: &str, params: Vec<Param>, ret: QualType, is_const: bool) -> Method {
    Method {
        name: name.to_owned(),
        params,
        return_type: ret,
        is_const,
        is_static: false,
        is_virtual: false,
        noexcept: false,
        location: loc(),
    }
}

fn class(name: &str, constructors: Vec<Constructor>, methods: Vec<Method>) -> Item {
    Item::Class(Class {
        name: name.to_owned(),
        namespace: vec!["geo".to_owned()],
        is_abstract: false,
        is_polymorphic: false,
        destructor: Destructor::Public { is_virtual: false },
        constructors,
        methods,
        unsupported_members: Vec::new(),
        location: loc(),
    })
}

fn ctor(params: Vec<Param>) -> Constructor {
    Constructor {
        params,
        noexcept: false,
        location: loc(),
    }
}

fn module(items: Vec<Item>) -> Module {
    Module {
        items,
        ..Module::new()
    }
}

/// Tek parametreli bir serbest fonksiyonun planı: bağlandıysa parametre tipi, atlandıysa neden.
fn single_param(ty: QualType) -> Result<BridgeType, String> {
    let plan = check(&module(vec![
        class("Point", Vec::new(), Vec::new()),
        function(&[], "f", vec![param("x", ty)], q(Type::Void)),
    ]));
    match plan.functions.as_slice() {
        [f] => Ok(f.params[0].ty.clone()),
        [] => Err(plan
            .skipped
            .iter()
            .find(|s| s.name == "f")
            .map(|s| s.reason.clone())
            .unwrap_or_default()),
        _ => panic!("beklenmeyen plan: {plan:?}"),
    }
}

fn free_return(ret: QualType) -> Result<Returns, String> {
    let plan = check(&module(vec![
        class("Point", Vec::new(), Vec::new()),
        function(&[], "f", Vec::new(), ret),
    ]));
    match plan.functions.as_slice() {
        [f] => Ok(f.returns.clone()),
        _ => Err(plan.skipped.iter().map(|s| s.reason.clone()).collect()),
    }
}

fn point_ref() -> TypeRef {
    TypeRef {
        rust_name: "Point".to_owned(),
        cpp_path: "::geo::Point".to_owned(),
    }
}

// ---- Tip kuralları ---------------------------------------------------------------------------

#[test]
fn portable_integers_map_to_rust_integers() {
    assert_eq!(single_param(q(i32_ty())), Ok(BridgeType::Int(IntType::I32)));
    assert_eq!(
        single_param(q(int("unsigned char", "unsigned char", 1, false, false))),
        Ok(BridgeType::Int(IntType::U8))
    );
    assert_eq!(
        single_param(q(int("std::int64_t", "long long", 8, true, true))),
        Ok(BridgeType::Int(IntType::I64))
    );
    assert_eq!(
        single_param(q(int("char", "char", 1, true, false))),
        Ok(BridgeType::Char)
    );
}

#[test]
fn platform_dependent_integers_are_rejected() {
    for (spelling, builtin) in [
        ("long", "long"),
        ("std::size_t", "unsigned long"),
        ("long long", "long long"),
    ] {
        let reason = single_param(q(int(spelling, builtin, 8, true, false))).unwrap_err();
        assert!(reason.contains("sabit genişlikli"), "{spelling}: {reason}");
    }
}

#[test]
fn pointers_need_ownership_annotations() {
    let reason = single_param(q(Type::Pointer {
        pointee: Box::new(q(i32_ty())),
    }))
    .unwrap_err();
    assert!(reason.contains("borrowed"), "{reason}");
}

#[test]
fn references_follow_the_type_table() {
    // const T& → &T
    assert_eq!(
        single_param(q(lref(konst(f64_ty())))),
        Ok(BridgeType::Ref {
            mutable: false,
            inner: Box::new(BridgeType::F64)
        })
    );
    // primitif T& → açıklama gerekiyor
    let reason = single_param(q(lref(q(f64_ty())))).unwrap_err();
    assert!(reason.contains("borrowed"), "{reason}");
    // sınıf T& → Pin<&mut T>
    assert_eq!(
        single_param(q(lref(q(record("Point"))))),
        Ok(BridgeType::Ref {
            mutable: true,
            inner: Box::new(BridgeType::Class(point_ref()))
        })
    );
    // std::string& → Pin<&mut CxxString>
    assert_eq!(
        single_param(q(lref(q(Type::String)))),
        Ok(BridgeType::Ref {
            mutable: true,
            inner: Box::new(BridgeType::String)
        })
    );
    // T&& → reddedilir
    assert!(
        single_param(q(Type::RValueRef {
            referent: Box::new(q(i32_ty()))
        }))
        .is_err()
    );
}

#[test]
fn classes_and_strings_by_value() {
    assert!(single_param(q(record("Point"))).is_err());
    assert!(single_param(q(Type::String)).is_err());
    assert_eq!(
        free_return(q(record("Point"))),
        Ok(Returns::BoxedClass(point_ref()))
    );
    assert_eq!(free_return(q(Type::String)), Ok(Returns::OwnedString));
    assert_eq!(free_return(q(Type::Void)), Ok(Returns::Unit));
}

#[test]
fn unknown_records_are_rejected() {
    let reason = single_param(q(lref(konst(record("Elsewhere"))))).unwrap_err();
    assert!(reason.contains("tanımlı değil"), "{reason}");
}

#[test]
fn unique_ptr_of_known_class_is_allowed() {
    let boxed = q(Type::UniquePtr {
        pointee: Box::new(q(record("Point"))),
    });
    assert_eq!(
        free_return(boxed.clone()),
        Ok(Returns::Value(BridgeType::UniquePtr(Box::new(
            BridgeType::Class(point_ref())
        ))))
    );
    assert!(single_param(boxed).is_ok());
}

#[test]
fn reference_returns_are_only_allowed_from_methods() {
    let point_ref_ty = q(lref(konst(record("Point"))));
    assert!(free_return(point_ref_ty.clone()).is_err());

    let plan = check(&module(vec![class(
        "Point",
        Vec::new(),
        vec![method("home", Vec::new(), point_ref_ty, true)],
    )]));
    let home = &plan.classes[0].methods[0];
    assert_eq!(
        home.returns,
        Returns::Value(BridgeType::Ref {
            mutable: false,
            inner: Box::new(BridgeType::Class(point_ref()))
        })
    );
}

#[test]
fn a_single_unsupported_param_skips_the_whole_function() {
    let plan = check(&module(vec![function(
        &[],
        "mix",
        vec![
            param("ok", q(i32_ty())),
            param(
                "bad",
                q(Type::Pointer {
                    pointee: Box::new(q(i32_ty())),
                }),
            ),
        ],
        q(Type::Void),
    )]));
    assert!(plan.functions.is_empty());
    assert_eq!(plan.skipped.len(), 1);
    assert!(
        plan.skipped[0].reason.contains("`bad`"),
        "{:?}",
        plan.skipped
    );
}

// ---- Sınıflar ve enum'lar ---------------------------------------------------------------------

#[test]
fn methods_get_receivers_and_constructors_become_factories() {
    let plan = check(&module(vec![class(
        "Vehicle",
        vec![ctor(vec![param("wheels", q(i32_ty()))])],
        vec![
            method("wheels", Vec::new(), q(i32_ty()), true),
            method(
                "drive",
                vec![param("km", q(f64_ty()))],
                q(Type::Void),
                false,
            ),
        ],
    )]));
    let vehicle = &plan.classes[0];
    assert_eq!(vehicle.type_ref.cpp_path, "::geo::Vehicle");
    assert_eq!(vehicle.constructors[0].rust_name, "Vehicle_new");
    assert!(vehicle.constructors[0].needs_shim());
    assert!(matches!(
        &vehicle.methods[0].callee,
        Callee::Method {
            receiver_mutable: false,
            ..
        }
    ));
    assert!(matches!(
        &vehicle.methods[1].callee,
        Callee::Method {
            receiver_mutable: true,
            ..
        }
    ));
    assert!(vehicle.methods[0].fallible);
}

#[test]
fn abstract_classes_get_no_factories() {
    let mut item = class("Shape", vec![ctor(Vec::new())], Vec::new());
    if let Item::Class(c) = &mut item {
        c.is_abstract = true;
    }
    let plan = check(&module(vec![item]));
    assert!(plan.classes[0].constructors.is_empty());
    assert!(plan.skipped.iter().any(|s| s.reason.contains("soyut")));
}

#[test]
fn classes_without_constructors_get_a_note() {
    let plan = check(&module(vec![class("Point", Vec::new(), Vec::new())]));
    assert_eq!(plan.classes.len(), 1);
    assert!(plan.notes.iter().any(|n| n.name == "geo::Point"));
}

#[test]
fn enums_use_their_underlying_type() {
    let plan = check(&module(vec![Item::Enum(Enum {
        name: "Level".to_owned(),
        namespace: vec!["io".to_owned()],
        scoped: true,
        underlying: int("std::uint8_t", "unsigned char", 1, false, true),
        variants: vec![EnumVariant {
            name: "High".to_owned(),
            value: 200,
        }],
        location: loc(),
    })]));
    assert_eq!(plan.enums[0].repr, IntType::U8);
    assert_eq!(plan.enums[0].type_ref.cpp_path, "::io::Level");
}

#[test]
fn unsupported_items_and_members_are_reported() {
    let mut item = class("Vehicle", Vec::new(), Vec::new());
    if let Item::Class(c) = &mut item {
        c.unsupported_members.push(Unsupported {
            construct: "field".to_owned(),
            name: "geo::Vehicle::count".to_owned(),
            reason: "alan".to_owned(),
            location: loc(),
        });
    }
    let plan = check(&module(vec![
        item,
        Item::Unsupported(Unsupported {
            construct: "variable".to_owned(),
            name: "g".to_owned(),
            reason: "değişken".to_owned(),
            location: loc(),
        }),
    ]));
    let names: Vec<&str> = plan.skipped.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"geo::Vehicle::count"), "{names:?}");
    assert!(names.contains(&"g"), "{names:?}");
}

// ---- Adlandırma -------------------------------------------------------------------------------

#[test]
fn overloads_get_type_suffixes() {
    let plan = check(&module(vec![
        function(
            &["geo"],
            "clamp",
            vec![param("v", q(i32_ty()))],
            q(i32_ty()),
        ),
        function(
            &["geo"],
            "clamp",
            vec![param("v", q(f64_ty()))],
            q(f64_ty()),
        ),
        function(&["geo"], "only", Vec::new(), q(Type::Void)),
    ]));
    let names: Vec<&str> = plan
        .functions
        .iter()
        .map(|f| f.rust_name.as_str())
        .collect();
    assert_eq!(names, ["clamp_i32", "clamp_f64", "only"]);
}

#[test]
fn same_name_in_different_namespaces_gets_a_prefix() {
    let plan = check(&module(vec![
        function(&["geo"], "length", Vec::new(), q(f64_ty())),
        function(&["physics"], "length", Vec::new(), q(f64_ty())),
    ]));
    let names: Vec<&str> = plan
        .functions
        .iter()
        .map(|f| f.rust_name.as_str())
        .collect();
    assert_eq!(names, ["geo_length", "physics_length"]);
}

#[test]
fn rust_keywords_are_escaped_and_unnamed_params_are_numbered() {
    let plan = check(&module(vec![function(
        &[],
        "type",
        vec![
            param("move", q(i32_ty())),
            Param {
                name: None,
                ty: q(i32_ty()),
            },
        ],
        q(Type::Void),
    )]));
    let f = &plan.functions[0];
    assert_eq!(f.rust_name, "type_");
    assert!(matches!(&f.callee, Callee::Free { cpp_name, .. } if cpp_name == "type"));
    let params: Vec<&str> = f.params.iter().map(|p| p.rust_name.as_str()).collect();
    assert_eq!(params, ["move_", "arg1"]);
}

#[test]
fn type_names_colliding_across_namespaces_are_prefixed() {
    let mut physics = class("Vehicle", Vec::new(), Vec::new());
    if let Item::Class(c) = &mut physics {
        c.namespace = vec!["physics".to_owned()];
    }
    let plan = check(&module(vec![
        class("Vehicle", Vec::new(), Vec::new()),
        physics,
    ]));
    let names: Vec<&str> = plan
        .classes
        .iter()
        .map(|c| c.type_ref.rust_name.as_str())
        .collect();
    assert_eq!(names, ["GeoVehicle", "PhysicsVehicle"]);
}

// ---- Sahiplik açıklamaları (Milestone 3) -----------------------------------------------------

fn ptr(pointee: QualType) -> QualType {
    q(Type::Pointer {
        pointee: Box::new(pointee),
    })
}

fn vector(element: Type) -> Type {
    Type::Vector {
        element: Box::new(q(element)),
    }
}

fn char_ty() -> Type {
    int("char", "char", 1, true, false)
}

fn with_dtor(mut item: Item, destructor: Destructor, polymorphic: bool) -> Item {
    if let Item::Class(c) = &mut item {
        c.destructor = destructor;
        c.is_polymorphic = polymorphic;
    }
    item
}

fn point_ty() -> BridgeType {
    BridgeType::Class(point_ref())
}

fn reference(mutable: bool, inner: BridgeType) -> BridgeType {
    BridgeType::Ref {
        mutable,
        inner: Box::new(inner),
    }
}

/// `f(x)` serbest fonksiyonu, `x` parametresi `borrowed` açıklamalı.
fn borrowed_param(ty: QualType) -> Result<ParamPlan, String> {
    let plan = check_with(
        &module(vec![
            class("Point", Vec::new(), Vec::new()),
            function(&[], "f", vec![param("x", ty)], q(Type::Void)),
        ]),
        "[functions.\"f\".params]\nx = \"borrowed\"\n",
    )
    .unwrap();
    match plan.functions.as_slice() {
        [f] => {
            assert!(
                plan.annotation_warnings.is_empty(),
                "{:?}",
                plan.annotation_warnings
            );
            Ok(f.params[0].clone())
        }
        _ => Err(plan.skipped[0].reason.clone()),
    }
}

#[test]
fn borrowed_pointer_params_become_references_passed_by_address() {
    let cases = [
        (ptr(konst(record("Point"))), reference(false, point_ty())),
        (ptr(q(record("Point"))), reference(true, point_ty())),
        (ptr(konst(f64_ty())), reference(false, BridgeType::F64)),
        (ptr(q(f64_ty())), reference(true, BridgeType::F64)),
        (ptr(q(Type::String)), reference(true, BridgeType::String)),
        (
            ptr(konst(vector(i32_ty()))),
            reference(
                false,
                BridgeType::Vector(Box::new(BridgeType::Int(IntType::I32))),
            ),
        ),
    ];
    for (ty, expected) in cases {
        let bound = borrowed_param(ty.clone()).unwrap();
        assert_eq!(bound.ty, expected, "{ty:?}");
        assert_eq!(bound.pass, Pass::AddressOf, "{ty:?}");
    }
}

#[test]
fn borrowed_mutable_primitive_refs_become_mut_refs_without_a_shim() {
    let bound = borrowed_param(q(lref(q(f64_ty())))).unwrap();
    assert_eq!(bound.ty, reference(true, BridgeType::F64));
    assert_eq!(bound.pass, Pass::Direct);
}

#[test]
fn some_pointers_stay_rejected_even_when_annotated() {
    for (ty, needle) in [
        (ptr(q(Type::Void)), "void*"),
        (ptr(konst(char_ty())), "C string"),
        (ptr(ptr(q(i32_ty()))), "T**"),
        (ptr(q(record("Elsewhere"))), "tanımlı değil"),
    ] {
        let reason = borrowed_param(ty).unwrap_err();
        assert!(reason.contains(needle), "{needle}: {reason}");
    }
}

#[test]
fn unannotated_pointers_produce_suggestions() {
    let plan = check(&module(vec![
        class("Point", Vec::new(), Vec::new()),
        function(
            &["geo"],
            "link",
            vec![
                param("a", ptr(konst(record("Point")))),
                param("out", q(lref(q(f64_ty())))),
            ],
            q(Type::Void),
        ),
    ]));
    assert!(plan.functions.is_empty());
    let targets: Vec<&SuggestionTarget> = plan.suggestions.iter().map(|s| &s.target).collect();
    assert_eq!(
        targets,
        [
            &SuggestionTarget::Param {
                name: "a".to_owned(),
                slice_lengths: Vec::new(),
            },
            &SuggestionTarget::Param {
                name: "out".to_owned(),
                slice_lengths: Vec::new(),
            },
        ]
    );
    assert_eq!(plan.suggestions[0].key, "geo::link");
    assert_eq!(plan.suggestions[0].cpp_type, "const geo::Point*");
    assert_eq!(
        plan.suggestions[0].signature,
        "geo::link(const geo::Point* a, double& out)"
    );
}

#[test]
fn no_suggestions_when_an_annotation_would_not_help() {
    // `int* p, long n` M5'ten beri `p = { slice = "n" }` ile bağlanabiliyor (bkz.
    // `pointer_next_to_an_integer_suggests_a_slice`); bilinmeyen bir sınıfın pointer'ı ise slice
    // olamaz, `long` da başka hiçbir açıklamayla düzelmez.
    let plan = check(&module(vec![function(
        &[],
        "f",
        vec![
            param("p", ptr(q(record("Unknown")))),
            param("n", q(int("long", "long", 8, true, false))),
        ],
        q(Type::Void),
    )]));
    assert!(plan.functions.is_empty());
    assert!(plan.suggestions.is_empty(), "{:?}", plan.suggestions);
}

/// `geo::make()` serbest fonksiyonu ya da `Point::get()` metodu, verilen dönüş açıklamasıyla.
fn annotated_return(
    ret: QualType,
    annotation: Option<&str>,
    method_const: Option<bool>,
    point: Item,
) -> Result<FunctionPlan, String> {
    let (items, key) = match method_const {
        None => (
            vec![point, function(&["geo"], "make", Vec::new(), ret)],
            "geo::make",
        ),
        Some(is_const) => {
            let mut point = point;
            if let Item::Class(c) = &mut point {
                c.methods.push(method("get", Vec::new(), ret, is_const));
            }
            (vec![point], "geo::Point::get")
        }
    };
    let toml = annotation.map_or_else(String::new, |a| {
        format!("[functions.\"{key}\"]\nreturns = \"{a}\"\n")
    });
    let plan = check_with(&module(items), &toml).unwrap();
    let bound = plan
        .functions
        .first()
        .or_else(|| plan.classes.first().and_then(|c| c.methods.first()));
    match bound {
        Some(f) => Ok(f.clone()),
        None => Err(plan
            .skipped
            .iter()
            .map(|s| s.reason.clone())
            .collect::<Vec<_>>()
            .join(" | ")),
    }
}

fn plain_point() -> Item {
    class("Point", Vec::new(), Vec::new())
}

#[test]
fn owned_pointer_returns_become_unique_ptrs() {
    let bound =
        annotated_return(ptr(q(record("Point"))), Some("owned"), None, plain_point()).unwrap();
    assert_eq!(bound.returns, Returns::OwnedPointer(point_ty()));
    assert!(bound.needs_shim());
    let bound = annotated_return(ptr(q(Type::String)), Some("owned"), None, plain_point()).unwrap();
    assert_eq!(bound.returns, Returns::OwnedPointer(BridgeType::String));
}

#[test]
fn owned_is_rejected_where_it_cannot_be_honoured() {
    for (ret, needle) in [
        (ptr(konst(record("Point"))), "const T*"),
        (ptr(q(i32_ty())), "UniquePtr<primitif>"),
        (q(lref(konst(record("Point")))), "Referans"),
    ] {
        let reason = annotated_return(ret, Some("owned"), Some(true), plain_point()).unwrap_err();
        assert!(reason.contains(needle), "{needle}: {reason}");
    }
}

#[test]
fn owned_pointer_returns_check_the_destructor() {
    let ret = || ptr(q(record("Point")));
    let cases = [
        (Destructor::NotPublic, false, Some("public değil")),
        (Destructor::Deleted, false, Some("silinmiş")),
        (
            Destructor::Public { is_virtual: false },
            true,
            Some("sanal değil"),
        ),
        (Destructor::Public { is_virtual: true }, true, None),
        (Destructor::Public { is_virtual: false }, false, None),
    ];
    for (destructor, polymorphic, rejection) in cases {
        let point = with_dtor(plain_point(), destructor, polymorphic);
        let result = annotated_return(ret(), Some("owned"), None, point);
        match rejection {
            Some(needle) => {
                let reason = result.unwrap_err();
                assert!(reason.contains(needle), "{destructor:?}: {reason}");
            }
            None => assert!(result.is_ok(), "{destructor:?}: {result:?}"),
        }
    }
}

#[test]
fn borrowed_pointer_returns_from_methods_follow_the_receiver() {
    let cases = [
        (ptr(q(record("Point"))), false, true),
        (ptr(q(record("Point"))), true, false),
        (ptr(konst(record("Point"))), false, false),
    ];
    for (ret, method_const, mutable) in cases {
        let bound =
            annotated_return(ret, Some("borrowed"), Some(method_const), plain_point()).unwrap();
        assert_eq!(
            bound.returns,
            Returns::BorrowedPointer {
                mutable,
                inner: point_ty()
            }
        );
        assert!(bound.fallible, "null kontrolü her zaman `Err` döndürebilir");
        assert!(bound.needs_shim());
    }
}

#[test]
fn borrowed_returns_are_rejected_for_free_functions() {
    let reason = annotated_return(
        ptr(q(record("Point"))),
        Some("borrowed"),
        None,
        plain_point(),
    )
    .unwrap_err();
    assert!(reason.contains("yalnızca metotlarda"), "{reason}");
}

#[test]
fn borrowed_mutable_primitive_ref_returns() {
    let ret = || q(lref(q(f64_ty())));
    let bound = annotated_return(ret(), Some("borrowed"), Some(false), plain_point()).unwrap();
    assert_eq!(
        bound.returns,
        Returns::Value(reference(true, BridgeType::F64))
    );
    assert!(annotated_return(ret(), Some("borrowed"), Some(true), plain_point()).is_err());
    assert!(annotated_return(ret(), None, Some(false), plain_point()).is_err());
}

#[test]
fn return_suggestions_offer_only_meaningful_options() {
    let options = |ret: QualType, method_const: Option<bool>| {
        let mut items = vec![plain_point()];
        match method_const {
            None => items.push(function(&["geo"], "make", Vec::new(), ret)),
            Some(is_const) => {
                if let Item::Class(c) = &mut items[0] {
                    c.methods.push(method("get", Vec::new(), ret, is_const));
                }
            }
        }
        let plan = check(&module(items));
        plan.suggestions
            .iter()
            .map(|s| match &s.target {
                SuggestionTarget::Return { options } => options.clone(),
                SuggestionTarget::Param { .. } => panic!("parametre önerisi beklenmiyordu"),
            })
            .collect::<Vec<_>>()
    };
    use ReturnAnnotation::{Borrowed, Owned};
    assert_eq!(options(ptr(q(record("Point"))), None), [vec![Owned]]);
    assert_eq!(
        options(ptr(q(record("Point"))), Some(false)),
        [vec![Owned, Borrowed]]
    );
    assert_eq!(options(ptr(q(f64_ty())), Some(true)), [vec![Borrowed]]);
    assert!(options(ptr(konst(record("Point"))), None).is_empty());
}

#[test]
fn vectors_follow_the_type_table() {
    let i32_vec = || BridgeType::Vector(Box::new(BridgeType::Int(IntType::I32)));
    assert_eq!(
        single_param(q(lref(konst(vector(i32_ty()))))),
        Ok(reference(false, i32_vec()))
    );
    assert_eq!(
        single_param(q(lref(q(vector(f64_ty()))))),
        Ok(reference(
            true,
            BridgeType::Vector(Box::new(BridgeType::F64))
        ))
    );
    assert_eq!(
        single_param(q(lref(konst(vector(record("Point")))))),
        Ok(reference(false, BridgeType::Vector(Box::new(point_ty()))))
    );
    assert!(single_param(q(vector(i32_ty()))).is_err());
    assert_eq!(
        free_return(q(vector(Type::String))),
        Ok(Returns::OwnedVector(BridgeType::String))
    );
    for (element, needle) in [
        (Type::Bool, "bit paketli"),
        (vector(i32_ty()), "eleman"),
        (char_ty(), "eleman"),
    ] {
        let reason = single_param(q(lref(konst(vector(element))))).unwrap_err();
        assert!(reason.contains(needle), "{needle}: {reason}");
    }
}

#[test]
fn classes_that_cannot_be_deleted_get_no_unique_ptrs() {
    let guarded = || {
        with_dtor(
            class("Point", vec![ctor(Vec::new())], Vec::new()),
            Destructor::NotPublic,
            false,
        )
    };
    let plan = check(&module(vec![guarded()]));
    assert!(plan.classes[0].constructors.is_empty());
    assert!(
        plan.skipped
            .iter()
            .any(|s| s.reason.contains("public değil")),
        "{:?}",
        plan.skipped
    );
    for ret in [
        q(record("Point")),
        q(Type::UniquePtr {
            pointee: Box::new(q(record("Point"))),
        }),
        q(vector(record("Point"))),
    ] {
        let plan = check(&module(vec![
            guarded(),
            function(&[], "f", Vec::new(), ret.clone()),
        ]));
        assert!(plan.functions.is_empty(), "{ret:?}");
    }
    // Referansla kullanmak silmeyi gerektirmez.
    let plan = check(&module(vec![
        guarded(),
        function(
            &[],
            "g",
            vec![param("p", q(lref(konst(record("Point")))))],
            q(Type::Void),
        ),
    ]));
    assert_eq!(plan.functions.len(), 1);
}

#[test]
fn constructor_params_are_annotated_with_the_constructor_name() {
    let plan = check_with(
        &module(vec![class(
            "Point",
            vec![ctor(vec![param("origin", ptr(konst(record("Point"))))])],
            Vec::new(),
        )]),
        "[functions.\"geo::Point::Point\".params]\norigin = \"borrowed\"\n",
    )
    .unwrap();
    assert_eq!(plan.classes[0].constructors.len(), 1);
    assert_eq!(
        plan.classes[0].constructors[0].params[0].pass,
        Pass::AddressOf
    );
}

#[test]
fn used_annotations_become_assumptions_with_file_name_only() {
    let plan = check_with(
        &module(vec![
            plain_point(),
            function(
                &["geo"],
                "make",
                vec![param("seed", ptr(konst(record("Point"))))],
                ptr(q(record("Point"))),
            ),
        ]),
        "[functions.\"geo::make\"]\nreturns = \"owned\"\n\n[functions.\"geo::make\".params]\nseed = \"borrowed\"\n",
    )
    .unwrap();
    let summary: Vec<(&str, &str, &str)> = plan
        .assumptions
        .iter()
        .map(|a| (a.subject.as_str(), a.annotation.as_str(), a.origin.as_str()))
        .collect();
    assert_eq!(
        summary,
        [
            ("parametre `seed`", "borrowed", "ferrobridge.toml:5"),
            ("dönüş", "owned", "ferrobridge.toml:2"),
        ]
    );
    assert!(plan.assumptions[1].promise.contains("delete"));
}

#[test]
fn annotations_that_match_nothing_are_errors() {
    let items = || {
        vec![
            class("Point", vec![ctor(Vec::new())], Vec::new()),
            function(&["geo"], "f", vec![param("a", q(i32_ty()))], q(Type::Void)),
            function(&["geo"], "f", vec![param("b", q(f64_ty()))], q(Type::Void)),
        ]
    };
    let errors = check_with(
        &module(items()),
        r#"[functions."geo::g"]
returns = "owned"

[functions."geo::f".params]
c = "borrowed"

[functions."geo::Point::Point"]
returns = "owned"
"#,
    )
    .unwrap_err();
    let shown: Vec<String> = errors.iter().map(ToString::to_string).collect();
    assert_eq!(shown.len(), 3, "{shown:#?}");
    assert!(shown[0].starts_with("ferrobridge.toml:8: "), "{shown:#?}");
    assert!(shown[0].contains("constructor"), "{shown:#?}");
    assert!(shown[1].starts_with("ferrobridge.toml:5: "), "{shown:#?}");
    assert!(shown[1].contains("`a`, `b`"), "{shown:#?}");
    assert!(shown[2].starts_with("ferrobridge.toml:1: "), "{shown:#?}");
    assert!(shown[2].contains("bulunamadı"), "{shown:#?}");

    // Bir overload'da bulunan parametre adı hata değildir.
    assert!(
        check_with(
            &module(items()),
            "[functions.\"geo::f\".params]\nb = \"borrowed\"\n"
        )
        .is_ok()
    );
}

#[test]
fn unnecessary_annotations_are_warnings() {
    let plan = check_with(
        &module(vec![function(
            &["geo"],
            "f",
            vec![param("x", q(i32_ty()))],
            q(f64_ty()),
        )]),
        "[functions.\"geo::f\"]\nreturns = \"owned\"\n[functions.\"geo::f\".params]\nx = \"borrowed\"\n",
    )
    .unwrap();
    assert_eq!(plan.functions.len(), 1);
    assert!(plan.assumptions.is_empty());
    let shown: Vec<String> = plan
        .annotation_warnings
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(shown.len(), 2, "{shown:#?}");
    assert!(
        shown.iter().all(|w| w.contains("gerektirmiyor")),
        "{shown:#?}"
    );
}

// ---- `suggest` taslağı -------------------------------------------------------------------------

/// Her tablo ve anahtar için ilk seçeneği açar (seçenekler aynı anahtara alternatiflerdir).
fn uncomment_first_choices(draft: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    let mut table = String::new();
    draft
        .lines()
        .map(|line| {
            if line.starts_with('[') {
                table = line.to_owned();
                return line.to_owned();
            }
            let Some(value) = line.strip_prefix("# ") else {
                return line.to_owned();
            };
            let Some((key, _)) = value.split_once(" = ") else {
                return line.to_owned();
            };
            // Açıklama başlığındaki örnekler (`#   { slice = "n" } ...`) anahtar değil.
            if key.is_empty() || key.contains(' ') {
                return line.to_owned();
            }
            let is_assignment = value.contains(" = \"") || value.contains(" = {");
            if !is_assignment || !seen.insert((table.clone(), key.to_owned())) {
                return line.to_owned();
            }
            value.to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn suggestion_draft_is_valid_toml_that_binds_once_uncommented() {
    let items = || {
        let mut point = plain_point();
        if let Item::Class(c) = &mut point {
            c.methods.push(method(
                "attach",
                vec![param("target", ptr(q(record("Point"))))],
                q(Type::Void),
                false,
            ));
            c.methods.push(method(
                "attach",
                vec![
                    param("target", ptr(q(record("Point")))),
                    param("weight", q(lref(q(f64_ty())))),
                ],
                q(Type::Void),
                false,
            ));
            c.methods
                .push(method("child", Vec::new(), ptr(q(record("Point"))), false));
        }
        vec![point]
    };
    let plan = check(&module(items()));
    assert!(plan.classes[0].methods.is_empty());
    let draft = render_suggestions(&plan);

    // Değerler yorum satırı: taslak olduğu gibi hiçbir şey vaat etmez.
    let as_is = Annotations::from_toml_str(&draft, "ferrobridge.toml").unwrap();
    assert!(
        as_is
            .functions
            .values()
            .all(|f| f.returns.is_none() && f.params.is_empty())
    );
    // Overload'lar tek tabloda birleşir.
    assert_eq!(
        draft
            .matches("[functions.\"geo::Point::attach\".params]")
            .count(),
        1,
        "{draft}"
    );
    assert!(draft.contains("# returns = \"owned\""), "{draft}");
    assert!(draft.contains("# returns = \"borrowed\""), "{draft}");

    let filled = uncomment_first_choices(&draft);
    let plan = check_with(&module(items()), &filled).unwrap();
    assert_eq!(plan.classes[0].methods.len(), 3, "{filled}");
    assert!(plan.suggestions.is_empty());
}

#[test]
fn empty_suggestion_draft_says_so() {
    let plan = check(&module(vec![plain_point()]));
    assert_eq!(render_suggestions(&plan), "# Açıklama gereken yer yok.\n");
}

#[test]
fn an_annotation_shared_by_overloads_is_warned_about() {
    let items = vec![
        plain_point(),
        function(
            &["geo"],
            "attach",
            vec![param("w", ptr(q(record("Point"))))],
            q(Type::Void),
        ),
        function(
            &["geo"],
            "attach",
            vec![
                param("w", ptr(q(record("Point")))),
                param("keep", q(Type::Bool)),
            ],
            q(Type::Void),
        ),
    ];
    let plan = check_with(
        &module(items),
        "[functions.\"geo::attach\".params]\nw = \"borrowed\"\n",
    )
    .unwrap();
    assert_eq!(plan.functions.len(), 2);
    let shown: Vec<String> = plan
        .annotation_warnings
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(shown.len(), 1, "{shown:#?}");
    assert!(shown[0].starts_with("ferrobridge.toml:1: "), "{shown:#?}");
    assert!(
        shown[0].contains("geo::attach(geo::Point* w, bool keep)"),
        "{shown:#?}"
    );
}

// ---- Slice açıklamaları (Milestone 5) --------------------------------------------------------

fn u8_ty() -> Type {
    int("uint8_t", "unsigned char", 1, false, true)
}

fn size_ty() -> Type {
    int("size_t", "unsigned long", 8, false, false)
}

fn float_ty() -> Type {
    Type::Float {
        spelling: "float".to_owned(),
        size_bytes: 4,
    }
}

fn slice(mutable: bool, element: BridgeType) -> BridgeType {
    BridgeType::Slice {
        mutable,
        element: Box::new(element),
    }
}

/// `f(params)` serbest fonksiyonu ve onun için bir parametre açıklama tablosu.
fn check_slice(params: Vec<Param>, table: &str) -> Result<BridgePlan, Vec<AnnotationIssue>> {
    check_with(
        &module(vec![function(&[], "f", params, q(Type::Void))]),
        &format!("[functions.\"f\".params]\n{table}"),
    )
}

#[test]
fn slice_annotation_turns_pointer_and_length_into_one_slice() {
    let plan = check_slice(
        vec![param("p", ptr(konst(u8_ty()))), param("len", q(u8_ty()))],
        "p = { slice = \"len\" }\n",
    )
    .unwrap();
    let f = &plan.functions[0];
    assert_eq!(f.params.len(), 1, "uzunluk Rust imzasından çıkmalı");
    assert_eq!(f.params[0].rust_name, "p");
    assert_eq!(f.params[0].ty, slice(false, BridgeType::Int(IntType::U8)));
    assert_eq!(f.params[0].pass, Pass::Slice);
    assert_eq!(
        f.lengths,
        vec![LengthParam {
            position: 1,
            slice: 0,
            cpp_type: "unsigned char".to_owned(),
        }]
    );
    assert!(f.needs_shim());
    let assumption = &plan.assumptions[0];
    assert_eq!(assumption.subject, "parametre `p`");
    assert_eq!(assumption.annotation, "{ slice = \"len\" }");
    assert!(
        assumption.promise.contains("`len` elemanlık"),
        "{assumption:?}"
    );
    assert_eq!(assumption.origin, "ferrobridge.toml:2");
}

#[test]
fn mutable_slices_and_lengths_rust_cannot_name() {
    // `size_t` Rust'a hiç görünmediği için uzunluk olabilir; `float*` → `&mut [f32]`.
    let plan = check_slice(
        vec![
            param("n", q(size_ty())),
            param("seed", q(i32_ty())),
            param("out", ptr(q(float_ty()))),
        ],
        "out = { slice = \"n\" }\n",
    )
    .unwrap();
    let f = &plan.functions[0];
    let names: Vec<&str> = f.params.iter().map(|p| p.rust_name.as_str()).collect();
    assert_eq!(names, ["seed", "out"]);
    assert_eq!(f.params[1].ty, slice(true, BridgeType::F32));
    assert_eq!(
        f.lengths,
        vec![LengthParam {
            position: 0,
            slice: 1,
            cpp_type: "unsigned long".to_owned(),
        }]
    );
}

#[test]
fn invalid_slice_annotations_are_errors_with_lines() {
    let p_u8 = || param("p", ptr(konst(u8_ty())));
    let cases: Vec<(Vec<Param>, &str, &str)> = vec![
        (
            vec![p_u8()],
            "p = { slice = \"len\" }\n",
            "`len` adında parametresi yok",
        ),
        (
            vec![p_u8(), param("len", q(f64_ty()))],
            "p = { slice = \"len\" }\n",
            "tamsayı",
        ),
        (
            vec![p_u8(), param("len", q(u8_ty()))],
            "p = { slice = \"p\" }\n",
            "kendisi",
        ),
        (
            vec![
                param("p", ptr(konst(record("Point")))),
                param("n", q(i32_ty())),
            ],
            "p = { slice = \"n\" }\n",
            "sayı",
        ),
        (
            vec![param("p", ptr(konst(char_ty()))), param("n", q(i32_ty()))],
            "p = { slice = \"n\" }\n",
            "uint8_t",
        ),
        (
            vec![param("p", q(i32_ty())), param("n", q(i32_ty()))],
            "p = { slice = \"n\" }\n",
            "pointer",
        ),
        (
            vec![
                p_u8(),
                param("q", ptr(konst(u8_ty()))),
                param("n", q(i32_ty())),
            ],
            "p = { slice = \"n\" }\nq = { slice = \"n\" }\n",
            "iki",
        ),
        (
            vec![p_u8(), param("n", q(i32_ty()))],
            "p = { slice = \"n\" }\nn = \"borrowed\"\n",
            "uzunluk",
        ),
    ];
    for (params, table, needle) in cases {
        let errors = check_slice(params, table).unwrap_err();
        let shown: Vec<String> = errors.iter().map(ToString::to_string).collect();
        assert!(
            shown
                .iter()
                .any(|e| e.starts_with("ferrobridge.toml:") && e.contains(needle)),
            "{table}: `{needle}` bekleniyordu: {shown:#?}"
        );
    }
}

#[test]
fn pointer_next_to_an_integer_suggests_a_slice() {
    let plan = check(&module(vec![function(
        &[],
        "crc",
        vec![
            param("buf", ptr(konst(u8_ty()))),
            param("size", q(size_ty())),
            param("flag", q(Type::Bool)),
        ],
        q(Type::Void),
    )]));
    assert_eq!(
        plan.suggestions[0].target,
        SuggestionTarget::Param {
            name: "buf".to_owned(),
            slice_lengths: vec!["size".to_owned()],
        }
    );
    let draft = render_suggestions(&plan);
    assert!(draft.contains("# buf = \"borrowed\""), "{draft}");
    assert!(draft.contains("# buf = { slice = \"size\" }"), "{draft}");
    // Bir sınıf pointer'ı için slice önerilmez.
    let plan = check(&module(vec![
        plain_point(),
        function(
            &[],
            "g",
            vec![
                param("p", ptr(konst(record("Point")))),
                param("n", q(i32_ty())),
            ],
            q(Type::Void),
        ),
    ]));
    assert_eq!(
        plan.suggestions[0].target,
        SuggestionTarget::Param {
            name: "p".to_owned(),
            slice_lengths: Vec::new(),
        }
    );
}

// ---- Sabitler (Milestone 5) -------------------------------------------------------------------

fn constant(ns: &[&str], name: &str, ty: Type, value: ConstantValue) -> Item {
    Item::Constant(ferrobridge_ir::Constant {
        name: name.to_owned(),
        namespace: ns.iter().map(|s| (*s).to_owned()).collect(),
        ty,
        value,
        location: loc(),
    })
}

#[test]
fn constants_get_rust_types_and_keep_their_values() {
    let i8_ty = int("int8_t", "signed char", 1, true, true);
    let u64_ty = int("uint64_t", "unsigned long long", 8, false, true);
    let plan = check(&module(vec![
        constant(&[], "WGS84_A", f64_ty(), ConstantValue::Float(6_378_137.0)),
        constant(
            &[],
            "GRAVITY",
            float_ty(),
            ConstantValue::Float(9.806_650_161_743_164),
        ),
        constant(&[], "OFFSET", i8_ty.clone(), ConstantValue::Signed(-5)),
        constant(&[], "ALL", u64_ty, ConstantValue::Unsigned(u64::MAX)),
        constant(&[], "ON", Type::Bool, ConstantValue::Bool(true)),
        constant(&[], "LETTER", char_ty(), ConstantValue::Signed(65)),
    ]));
    let summary: Vec<(&str, &BridgeType, ConstantValue)> = plan
        .constants
        .iter()
        .map(|c| (c.rust_name.as_str(), &c.ty, c.value))
        .collect();
    assert_eq!(
        summary,
        [
            (
                "WGS84_A",
                &BridgeType::F64,
                ConstantValue::Float(6_378_137.0)
            ),
            (
                "GRAVITY",
                &BridgeType::F32,
                ConstantValue::Float(9.806_650_161_743_164)
            ),
            (
                "OFFSET",
                &BridgeType::Int(IntType::I8),
                ConstantValue::Signed(-5)
            ),
            (
                "ALL",
                &BridgeType::Int(IntType::U64),
                ConstantValue::Unsigned(u64::MAX)
            ),
            ("ON", &BridgeType::Bool, ConstantValue::Bool(true)),
            ("LETTER", &BridgeType::Char, ConstantValue::Signed(65)),
        ]
    );
    assert!(plan.skipped.is_empty(), "{:?}", plan.skipped);
}

#[test]
fn constants_that_cannot_be_written_portably_are_skipped() {
    let plan = check(&module(vec![
        constant(
            &[],
            "BIG",
            int("long", "long", 8, true, false),
            ConstantValue::Signed(1),
        ),
        constant(&[], "HIGH", char_ty(), ConstantValue::Unsigned(200)),
        constant(
            &[],
            "WRONG",
            int("int8_t", "signed char", 1, true, true),
            ConstantValue::Signed(300),
        ),
        constant(&["(anonim)"], "HIDDEN", i32_ty(), ConstantValue::Signed(1)),
    ]));
    assert!(plan.constants.is_empty(), "{:?}", plan.constants);
    let reasons: Vec<(&str, &str)> = plan
        .skipped
        .iter()
        .map(|s| (s.name.as_str(), s.reason.as_str()))
        .collect();
    assert!(reasons[0].1.contains("platformlar arasında"), "{reasons:?}");
    assert!(reasons[1].1.contains("0–127"), "{reasons:?}");
    assert!(reasons[2].1.contains("sığmıyor"), "{reasons:?}");
    assert!(reasons[3].1.contains("Anonim namespace"), "{reasons:?}");
}

#[test]
fn constant_names_avoid_collisions() {
    let plan = check(&module(vec![
        constant(&["metric"], "UNIT", f64_ty(), ConstantValue::Float(1.0)),
        constant(
            &["imperial"],
            "UNIT",
            f64_ty(),
            ConstantValue::Float(0.3048),
        ),
        constant(&[], "type", i32_ty(), ConstantValue::Signed(1)),
        constant(&[], "half", i32_ty(), ConstantValue::Signed(2)),
        function(&[], "half", vec![param("x", q(i32_ty()))], q(i32_ty())),
    ]));
    let names: Vec<&str> = plan
        .constants
        .iter()
        .map(|c| c.rust_name.as_str())
        .collect();
    assert_eq!(names, ["metric_UNIT", "imperial_UNIT", "type_"]);
    assert_eq!(plan.skipped.len(), 1);
    assert!(
        plan.skipped[0].reason.contains("aynı ad alanında"),
        "{:?}",
        plan.skipped
    );
}
