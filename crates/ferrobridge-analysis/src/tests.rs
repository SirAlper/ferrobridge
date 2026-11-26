//! Bağlanabilirlik kuralları ve adlandırma için tablo testleri.

use ferrobridge_ir::{
    Class, Constructor, Enum, EnumVariant, Function, Item, Method, Module, Param, QualType,
    SourceLocation, Type, Unsupported,
};

use super::*;

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
    assert!(reason.contains("Milestone 3"), "{reason}");
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
    assert!(reason.contains("Milestone 3"), "{reason}");
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
