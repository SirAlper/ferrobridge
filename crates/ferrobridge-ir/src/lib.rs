//! ferrobridge's language-independent intermediate model (IR).
//!
//! The data structures produced by the parser and consumed by the analysis and code generation
//! layers are defined here. This crate depends on no other ferrobridge crate; even if the parser
//! changes, the IR stays the same (see `docs/adr/0001-libclang-via-clang-sys.md`).

mod item;
mod types;

pub use item::{
    Class, Constant, ConstantValue, Constructor, Destructor, Enum, EnumVariant, Field, Function,
    Item, Layout, Method, Param, SourceLocation, TemplateInstance, Unsupported,
};
pub use types::{QualType, Type};

use serde::{Deserialize, Serialize};

/// The version of the IR's JSON format. Incremented when the format changes incompatibly.
pub const IR_FORMAT_VERSION: u32 = 5;

/// The root of the symbols read from a C++ header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Module {
    /// The IR format version this module was produced with.
    pub format_version: u32,
    /// The items found in the header, in source order.
    pub items: Vec<Item>,
}

impl Module {
    /// Creates an empty module with the current format version.
    pub fn new() -> Self {
        Self {
            format_version: IR_FORMAT_VERSION,
            items: Vec::new(),
        }
    }
}

impl Default for Module {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn location(line: u32) -> SourceLocation {
        SourceLocation {
            file: "input.h".to_owned(),
            line,
            column: 1,
        }
    }

    fn int32() -> QualType {
        QualType {
            is_const: false,
            ty: Type::Integer {
                spelling: "std::int32_t".to_owned(),
                builtin: "int".to_owned(),
                size_bytes: 4,
                signed: true,
                fixed_width: true,
            },
        }
    }

    fn plain(ty: Type) -> QualType {
        QualType {
            is_const: false,
            ty,
        }
    }

    #[test]
    fn classes_enums_and_std_types_round_trip_through_json() {
        let vehicle = Type::Record {
            name: "Vehicle".to_owned(),
            namespace: vec!["geo".to_owned()],
            declared_in: Some("vehicle.h".to_owned()),
        };
        let module = Module {
            format_version: IR_FORMAT_VERSION,
            items: vec![
                Item::Class(Class {
                    name: "Vehicle".to_owned(),
                    namespace: vec!["geo".to_owned()],
                    is_abstract: false,
                    is_polymorphic: true,
                    destructor: Destructor::Public { is_virtual: true },
                    constructors: vec![Constructor {
                        params: vec![Param {
                            name: Some("wheels".to_owned()),
                            ty: int32(),
                        }],
                        noexcept: false,
                        location: location(4),
                    }],
                    methods: vec![Method {
                        name: "rename".to_owned(),
                        params: vec![Param {
                            name: Some("name".to_owned()),
                            ty: QualType {
                                is_const: false,
                                ty: Type::LValueRef {
                                    referent: Box::new(QualType {
                                        is_const: true,
                                        ty: Type::String,
                                    }),
                                },
                            },
                        }],
                        return_type: plain(Type::UniquePtr {
                            pointee: Box::new(plain(vehicle.clone())),
                        }),
                        is_const: false,
                        is_static: false,
                        is_virtual: true,
                        noexcept: false,
                        operator: None,
                        location: location(5),
                    }],
                    unsupported_members: vec![Unsupported {
                        construct: "operator".to_owned(),
                        name: "geo::Vehicle::operator==".to_owned(),
                        reason: "operator".to_owned(),
                        location: location(6),
                    }],
                    fields: vec![Field {
                        name: "count".to_owned(),
                        ty: int32(),
                        public: true,
                        offset_bits: Some(64),
                        location: location(7),
                    }],
                    layout: Some(Layout {
                        size_bytes: 16,
                        align_bytes: 8,
                    }),
                    user_copy_or_move: true,
                    instance_of: Some(TemplateInstance {
                        template: "Vehicle".to_owned(),
                        spelling: "geo::Vehicle<int>".to_owned(),
                    }),
                    location: location(3),
                }),
                Item::Enum(Enum {
                    name: "Mode".to_owned(),
                    namespace: vec!["geo".to_owned()],
                    scoped: true,
                    underlying: int32().ty,
                    variants: vec![
                        EnumVariant {
                            name: "Manual".to_owned(),
                            value: 0,
                        },
                        EnumVariant {
                            name: "Auto".to_owned(),
                            value: -5,
                        },
                    ],
                    location: location(8),
                }),
                Item::Function(Function {
                    name: "mode_of".to_owned(),
                    namespace: Vec::new(),
                    params: vec![
                        Param {
                            name: None,
                            ty: plain(vehicle),
                        },
                        Param {
                            name: Some("ids".to_owned()),
                            ty: plain(Type::Vector {
                                element: Box::new(int32()),
                            }),
                        },
                    ],
                    return_type: plain(Type::Enum {
                        name: "Mode".to_owned(),
                        namespace: vec!["geo".to_owned()],
                        declared_in: None,
                    }),
                    noexcept: true,
                    location: location(9),
                }),
            ],
        };
        assert_eq!(round_trip(&module), module);
    }

    fn round_trip(module: &Module) -> Module {
        let json = serde_json::to_string(module).unwrap();
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn empty_module_round_trips_through_json() {
        let module = Module::new();
        assert_eq!(round_trip(&module), module);
        assert_eq!(module.format_version, IR_FORMAT_VERSION);
    }

    #[test]
    fn every_item_and_type_variant_round_trips_through_json() {
        let module = Module {
            format_version: IR_FORMAT_VERSION,
            items: vec![
                Item::Function(Function {
                    name: "f".to_owned(),
                    namespace: vec!["geo".to_owned(), "detail".to_owned()],
                    params: vec![
                        Param {
                            name: Some("a".to_owned()),
                            ty: int32(),
                        },
                        Param {
                            name: None,
                            ty: QualType {
                                is_const: false,
                                ty: Type::Pointer {
                                    pointee: Box::new(QualType {
                                        is_const: true,
                                        ty: Type::Float {
                                            spelling: "double".to_owned(),
                                            size_bytes: 8,
                                        },
                                    }),
                                },
                            },
                        },
                        Param {
                            name: Some("r".to_owned()),
                            ty: QualType {
                                is_const: false,
                                ty: Type::LValueRef {
                                    referent: Box::new(QualType {
                                        is_const: false,
                                        ty: Type::Bool,
                                    }),
                                },
                            },
                        },
                        Param {
                            name: Some("m".to_owned()),
                            ty: QualType {
                                is_const: false,
                                ty: Type::RValueRef {
                                    referent: Box::new(int32()),
                                },
                            },
                        },
                        Param {
                            name: Some("p".to_owned()),
                            ty: QualType {
                                is_const: false,
                                ty: Type::Unsupported {
                                    spelling: "Point".to_owned(),
                                    reason: "class".to_owned(),
                                },
                            },
                        },
                    ],
                    return_type: QualType {
                        is_const: false,
                        ty: Type::Void,
                    },
                    noexcept: true,
                    location: location(3),
                }),
                Item::Unsupported(Unsupported {
                    construct: "class".to_owned(),
                    name: "Vehicle".to_owned(),
                    reason: "Milestone 2".to_owned(),
                    location: location(5),
                }),
            ],
        };
        assert_eq!(round_trip(&module), module);
    }

    #[test]
    fn json_uses_readable_tags() {
        let json = serde_json::to_value(Item::Function(Function {
            name: "g".to_owned(),
            namespace: Vec::new(),
            params: Vec::new(),
            return_type: int32(),
            noexcept: false,
            location: location(1),
        }))
        .unwrap();
        assert_eq!(json["kind"], "function");
        assert_eq!(json["return_type"]["type"]["kind"], "integer");
        assert_eq!(json["return_type"]["const"], false);
    }

    #[test]
    fn constants_round_trip_and_use_readable_tags() {
        let constants = [
            (int32().ty, ConstantValue::Signed(-5), "signed"),
            (
                Type::Integer {
                    spelling: "uint64_t".to_owned(),
                    builtin: "unsigned long long".to_owned(),
                    size_bytes: 8,
                    signed: false,
                    fixed_width: true,
                },
                ConstantValue::Unsigned(u64::MAX),
                "unsigned",
            ),
            (
                Type::Float {
                    spelling: "double".to_owned(),
                    size_bytes: 8,
                },
                ConstantValue::Float(6_378_137.0),
                "float",
            ),
            (Type::Bool, ConstantValue::Bool(true), "bool"),
        ];
        for (ty, value, tag) in constants {
            let item = Item::Constant(Constant {
                name: "K".to_owned(),
                namespace: vec!["units".to_owned()],
                ty,
                value,
                location: location(2),
            });
            let json = serde_json::to_value(&item).unwrap();
            assert_eq!(json["kind"], "constant");
            assert_eq!(json["value"]["kind"], tag);
            assert_eq!(serde_json::from_value::<Item>(json).unwrap(), item);
        }
    }

    #[test]
    fn destructors_and_vectors_use_readable_tags() {
        let destructors = [
            (Destructor::Public { is_virtual: false }, "public"),
            (Destructor::NotPublic, "not_public"),
            (Destructor::Deleted, "deleted"),
        ];
        for (destructor, tag) in destructors {
            let json = serde_json::to_value(destructor).unwrap();
            assert_eq!(json["kind"], tag);
            assert_eq!(
                serde_json::from_value::<Destructor>(json).unwrap(),
                destructor
            );
        }
        let vector = Type::Vector {
            element: Box::new(int32()),
        };
        let json = serde_json::to_value(&vector).unwrap();
        assert_eq!(json["kind"], "vector");
        assert_eq!(json["element"]["type"]["kind"], "integer");
    }
}
