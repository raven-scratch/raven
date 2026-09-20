//! raven's type model, and how it meets the block catalog's shapes.
//!
//! Scratch has one runtime value type — a string that is usually numeric — and
//! two block shapes: round and hexagonal. raven's four types are a *discipline*
//! laid over that, not a different target, so the whole of this module is a
//! mapping between a [`Ty`] and a catalog [`Shape`].
//!
//! Two consequences worth stating where the code is:
//!
//! * the conversions that exist are free, because the target does not need them:
//!   `num(x)` and `str(x)` change what the checker permits and emit no block;
//! * `bool` has no conversion in either direction, because no such conversion has
//!   a single meaning and the block would have to guess.

use std::sync::{Mutex, OnceLock};

use raven_scratch::catalog::Shape;

/// A declared `struct`, by an interned id.
///
/// A [`Ty`] is `Copy`, so a struct type carries a number rather than a name.
/// The number is the index of the name in one process-wide, append-only table:
/// the same name always interns to the same id, so two files that both declare
/// or use `Point` agree without the parser having to see both.
pub type StructId = u32;

fn struct_names() -> &'static Mutex<Vec<&'static str>> {
    static NAMES: OnceLock<Mutex<Vec<&'static str>>> = OnceLock::new();
    NAMES.get_or_init(|| Mutex::new(Vec::new()))
}

/// The id of `name`, interning it the first time it is seen.
#[must_use]
pub fn intern_struct(name: &str) -> StructId {
    let mut names = struct_names()
        .lock()
        .expect("the struct table is not poisoned");
    if let Some(index) = names.iter().position(|n| *n == name) {
        return index as StructId;
    }
    let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
    names.push(leaked);
    (names.len() - 1) as StructId
}

/// The name an id stands for.
#[must_use]
pub fn struct_name(id: StructId) -> &'static str {
    struct_names()
        .lock()
        .expect("the struct table is not poisoned")
        .get(id as usize)
        .copied()
        .unwrap_or("<unknown struct>")
}

/// One of the three scalar types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scalar {
    Num,
    Str,
    Bool,
}

impl Scalar {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Scalar::Num => "num",
            Scalar::Str => "str",
            Scalar::Bool => "bool",
        }
    }

    /// The reporter Scratch uses to read a `proc` parameter of this type, as it
    /// appears in the catalog.
    #[must_use]
    pub const fn argument_reporter(self) -> &'static str {
        match self {
            Scalar::Bool => "argument_reporter_boolean",
            Scalar::Num | Scalar::Str => "argument_reporter_string_number",
        }
    }

    /// The `proccode` letter Scratch stores in a custom block's mutation.
    #[must_use]
    pub const fn proccode_letter(self) -> char {
        match self {
            Scalar::Str => 's',
            Scalar::Num => 'n',
            Scalar::Bool => 'b',
        }
    }
}

/// A raven type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    Num,
    Str,
    Bool,
    /// `list<T>`. Scratch lists are untyped; raven's element type is a promise
    /// the checker keeps on your behalf.
    List(Scalar),
    /// `map<K, V>` — a key/value table, one Scratch list of alternating pairs.
    Map(Scalar, Scalar),
    /// A declared `struct`: a fixed frame of named fields in the VMS.
    Struct(StructId),
}

impl Ty {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Ty::Num => "num",
            Ty::Str => "str",
            Ty::Bool => "bool",
            Ty::List(Scalar::Num) => "list<num>",
            Ty::List(Scalar::Str) => "list<str>",
            Ty::List(Scalar::Bool) => "list<bool>",
            Ty::Map(Scalar::Str, Scalar::Num) => "map<str, num>",
            Ty::Map(Scalar::Str, Scalar::Str) => "map<str, str>",
            Ty::Map(Scalar::Num, Scalar::Num) => "map<num, num>",
            Ty::Map(Scalar::Num, Scalar::Str) => "map<num, str>",
            Ty::Map(..) => "map<?, ?>",
            Ty::Struct(id) => struct_name(id),
        }
    }

    /// The type a list of this type holds.
    #[must_use]
    pub const fn element(self) -> Option<Scalar> {
        match self {
            Ty::List(element) => Some(element),
            _ => None,
        }
    }

    /// Whether the type is stored in a scratch list rather than a scalar slot.
    #[must_use]
    pub const fn is_list(self) -> bool {
        matches!(self, Ty::List(_) | Ty::Map(..))
    }

    /// Whether the type is a scalar: one cell, one value.
    #[must_use]
    pub const fn is_scalar(self) -> bool {
        matches!(self, Ty::Num | Ty::Str | Ty::Bool)
    }

    /// Whether a value of this type can be compared, copied and stored in one
    /// cell. A struct is a *place*, not a value, so it is not.
    #[must_use]
    pub const fn is_place(self) -> bool {
        matches!(self, Ty::Struct(_))
    }
}

impl From<Scalar> for Ty {
    fn from(scalar: Scalar) -> Self {
        match scalar {
            Scalar::Num => Ty::Num,
            Scalar::Str => Ty::Str,
            Scalar::Bool => Ty::Bool,
        }
    }
}

/// Whether a value of type `ty` may fill a slot of shape `shape`.
///
/// This is the whole of raven's shape checking, and it is deliberately
/// conservative: a `num` fits a text slot because Scratch parses it, but a `str`
/// does not fit a numeric slot even though Scratch would also parse that, because
/// the day it does not parse is the day the bug ships.
///
/// `Variable`, `List`, `Broadcast`, `Menu` and `ParamName` are *mention* slots:
/// they name a declared thing rather than taking a value, and the resolver
/// handles them. A value type never fills one.
#[must_use]
pub fn fits(shape: Shape, ty: Ty) -> bool {
    match shape {
        Shape::Number | Shape::Positive | Shape::Whole | Shape::Integer | Shape::Angle => {
            ty == Ty::Num
        }
        Shape::Text => matches!(ty, Ty::Num | Ty::Str),
        Shape::Bool => ty == Ty::Bool,
        // A colour is a `"#rrggbb"` literal; `str` is the closest type, and the
        // literal form is checked separately.
        Shape::Color => ty == Ty::Str,
        Shape::Variable | Shape::List | Shape::Broadcast | Shape::Menu(_) | Shape::ParamName => {
            false
        }
    }
}

/// The conversions raven has.
///
/// There is exactly one kind, and it is free: the two directions Scratch itself
/// performs in a slot. Every other pair of types is a compile error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conversion {
    /// Retypes the value without emitting a block.
    Free,
}

/// The conversion from `from` to `to`, if raven has one.
#[must_use]
pub fn conversion(from: Ty, to: Ty) -> Option<Conversion> {
    match (from, to) {
        (Ty::Num, Ty::Str) | (Ty::Str, Ty::Num) => Some(Conversion::Free),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_fit_numeric_slots_and_not_boolean_ones() {
        assert!(fits(Shape::Number, Ty::Num));
        assert!(!fits(Shape::Number, Ty::Str));
        assert!(!fits(Shape::Bool, Ty::Num));
        assert!(fits(Shape::Bool, Ty::Bool));
    }

    #[test]
    fn text_slots_take_both_scalars_but_numbers_stay_strict() {
        assert!(fits(Shape::Text, Ty::Num));
        assert!(fits(Shape::Text, Ty::Str));
        assert!(!fits(Shape::Text, Ty::Bool));
    }

    #[test]
    fn mention_slots_never_take_a_value() {
        for shape in [
            Shape::Variable,
            Shape::List,
            Shape::Broadcast,
            Shape::Menu("math_op"),
            Shape::ParamName,
        ] {
            for ty in [Ty::Num, Ty::Str, Ty::Bool, Ty::List(Scalar::Num)] {
                assert!(!fits(shape, ty), "{shape:?} accepted {ty:?}");
            }
        }
    }

    #[test]
    fn only_the_two_free_conversions_exist() {
        assert_eq!(conversion(Ty::Num, Ty::Str), Some(Conversion::Free));
        assert_eq!(conversion(Ty::Str, Ty::Num), Some(Conversion::Free));
        assert_eq!(conversion(Ty::Bool, Ty::Num), None);
        assert_eq!(conversion(Ty::Num, Ty::Bool), None);
        assert_eq!(conversion(Ty::List(Scalar::Num), Ty::Num), None);
    }

    #[test]
    fn list_types_read_back() {
        assert_eq!(Ty::List(Scalar::Str).name(), "list<str>");
        assert_eq!(Ty::List(Scalar::Str).element(), Some(Scalar::Str));
        assert!(Ty::List(Scalar::Bool).is_list());
        assert_eq!(Scalar::Bool.proccode_letter(), 'b');
    }
}
