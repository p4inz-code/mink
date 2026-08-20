//! Type-error model.
//!
//! Type errors are produced by the type checker when a program violates a
//! typing rule: a value of one type is used where another is required, an
//! operator is applied to incompatible operands, a range is built from
//! invalid endpoints, a value that is not a function is called, a call
//! supplies the wrong number of arguments, or a loop iterates over a value
//! that is not a range.
//!
//! The model mirrors the lexer, parser, and semantic error designs: each
//! error carries a stable category ([`TypeErrorKind`]), the precise source
//! [`Span`](crate::source::Span) it applies to, rendered expected/actual
//! types where useful, the offending operator for operator errors, and an
//! optional related span (for example the target of a mismatched
//! assignment). Codes `E-T01` … `E-T06` continue the established ranges
//! (`E-L*` lexical, `E-P*` syntax, `E-S*` semantic); the full catalog is in
//! `docs/implementation/TYPE_SYSTEM_IMPLEMENTATION.md`.

use std::fmt;

use crate::source::Span;

/// The category of a type error.
///
/// Every category has a stable machine-readable code
/// ([`TypeErrorKind::code`]) and a human-readable message
/// ([`fmt::Display`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeErrorKind {
    /// A value of one type is used where a different type is required
    /// (assignment, call argument, return value, or boolean condition).
    TypeMismatch,
    /// A unary or binary operator is applied to incompatible operands.
    InvalidOperator,
    /// A range is constructed from incompatible endpoints.
    InvalidRange,
    /// A value that does not have a function type is called.
    NotCallable,
    /// A call supplies the wrong number of arguments.
    WrongArgumentCount,
    /// A `for` loop iterates over a value that is not a range.
    NotIterable,
    /// A member is accessed on a value whose type is not a struct.
    MemberAccessOnNonStruct,
    /// A struct has no field with the accessed name.
    UnknownMember,
    /// A value whose type is not an array is indexed.
    IndexOnNonArray,
    /// An array is indexed by a value that is not an `Int`.
    InvalidIndexType,
    /// An array is indexed by a constant that is out of bounds.
    IndexOutOfRange,
    /// A struct literal initializes a field the struct does not declare.
    UnknownStructField,
    /// A struct literal omits a declared field.
    MissingStructField,
    /// A struct literal initializes the same field twice.
    DuplicateFieldInit,
    /// A type name does not denote a known type.
    UnknownType,
    /// An array type's length is invalid (not a positive literal, or its
    /// size overflows the runtime model).
    InvalidArrayLength,
    /// An array literal has no elements, so its element type cannot be
    /// inferred.
    EmptyArrayLiteral,
    /// An aggregate's layout is invalid: a recursive struct, an empty
    /// struct, or a value larger than the runtime memory model.
    InvalidAggregateLayout,
    /// A reference is taken of something that cannot be borrowed (a
    /// non-place, an existing reference, or a deref-rooted place — the
    /// borrow/reborrow forms outside the Session 16 model).
    InvalidBorrowTarget,
    /// A value that is not a reference is dereferenced with `*`.
    DerefNonReference,
    /// An assignment is made through an immutable reference (`*r = v`
    /// where `r: &T`); only `&mut T` allows writes through it.
    AssignThroughImmutableRef,
    /// A variant path's first segment names a type that is not an enum
    /// (e.g. `Int::Foo` or a struct name), so it has no variants.
    NotAnEnum,
    /// An enum has no variant with the accessed name.
    UnknownVariant,
    /// A `match` statement is not exhaustive: some value of the
    /// scrutinee's type matches no arm (missing variants, or a finite
    /// domain without a catch-all).
    NonExhaustiveMatch,
    /// A `match` arm can never run: it follows a `_`/binding arm that
    /// already matches every value, or repeats a pattern an earlier arm
    /// already matches.
    UnreachableMatchArm,
    /// A `match` scrutinee has a type that cannot be matched on (only
    /// `Int`, `Bool`, and enums are matchable in this milestone).
    InvalidMatchScrutinee,
    /// A data-carrying variant's payload type is not a supported value
    /// type (a pointer, reference, array, or function type).
    InvalidVariantPayload,
    /// A data-carrying variant is constructed with a payload whose type
    /// does not match the variant's declared payload type.
    VariantPayloadMismatch,
    /// A unit variant is constructed or matched with a payload, or a
    /// data-carrying variant is constructed without one.
    VariantPayloadArity,
    /// Two enum values are compared with `==`/`!=`. Enum equality is not
    /// supported (unit variants compare by discriminant; tagged-union
    /// equality is deferred).
    EnumEquality,
    /// Two variants of one enum share the same discriminant value, so the
    /// tag word cannot distinguish them.
    DuplicateDiscriminant,
    /// An implicit variant discriminant would continue past the largest
    /// 64-bit value.
    DiscriminantOverflow,
    /// An assignment targets a member or element of a dereferenced
    /// reference (`(*r).x = v`). Only whole-value deref assignment
    /// (`*r = v`) is in the reference model; deref-rooted places are
    /// deferred, so this is rejected instead of silently writing to a
    /// temporary copy.
    DerefRootedAssignment,
    /// An or-pattern's alternatives do not all bind the same names
    /// (session 27): a name bound in one alternative must be bound in
    /// every alternative, with a compatible type.
    InvalidOrPattern,
    /// A tuple field access (session 29) uses an index that is out of
    /// range for the tuple type.
    TupleIndexOutOfRange,
    /// A `break;` inside a loop expression (session 30) does not carry a
    /// value, but the loop is used in expression position and must
    /// produce one.
    BreakValueExpected,
    /// A tuple destructuring pattern (session 31) is applied to a
    /// non-tuple type.
    CannotDestructure,
    /// A tuple destructuring pattern (session 31) has a different number
    /// of elements than the tuple type.
    DestructureArityMismatch,
}

impl TypeErrorKind {
    /// Stable machine-readable code for this error category.
    ///
    /// Codes are provisional until the full diagnostic engine defines the
    /// final error-code namespace, matching the lexer/parser/semantic
    /// convention.
    pub fn code(self) -> &'static str {
        match self {
            Self::TypeMismatch => "E-T01",
            Self::InvalidOperator => "E-T02",
            Self::InvalidRange => "E-T03",
            Self::NotCallable => "E-T04",
            Self::WrongArgumentCount => "E-T05",
            Self::NotIterable => "E-T06",
            Self::MemberAccessOnNonStruct => "E-T07",
            Self::UnknownMember => "E-T08",
            Self::IndexOnNonArray => "E-T09",
            Self::InvalidIndexType => "E-T10",
            Self::IndexOutOfRange => "E-T11",
            Self::UnknownStructField => "E-T12",
            Self::MissingStructField => "E-T13",
            Self::DuplicateFieldInit => "E-T14",
            Self::UnknownType => "E-T15",
            Self::InvalidArrayLength => "E-T16",
            Self::EmptyArrayLiteral => "E-T17",
            Self::InvalidAggregateLayout => "E-T18",
            Self::InvalidBorrowTarget => "E-T19",
            Self::DerefNonReference => "E-T20",
            Self::AssignThroughImmutableRef => "E-T21",
            Self::NotAnEnum => "E-T22",
            Self::UnknownVariant => "E-T23",
            Self::NonExhaustiveMatch => "E-T24",
            Self::UnreachableMatchArm => "E-T25",
            Self::InvalidMatchScrutinee => "E-T26",
            Self::InvalidVariantPayload => "E-T27",
            Self::VariantPayloadMismatch => "E-T28",
            Self::VariantPayloadArity => "E-T29",
            Self::EnumEquality => "E-T30",
            Self::DuplicateDiscriminant => "E-T31",
            Self::DiscriminantOverflow => "E-T32",
            Self::DerefRootedAssignment => "E-T33",
            Self::InvalidOrPattern => "E-T34",
            Self::TupleIndexOutOfRange => "E-T35",
            Self::BreakValueExpected => "E-T36",
            Self::CannotDestructure => "E-T37",
            Self::DestructureArityMismatch => "E-T38",
        }
    }
}

/// A single type error: a category, the span it applies to, rendered
/// expected/actual types where meaningful, the offending operator (for
/// operator errors), and an optional related span pointing at a second
/// location involved in the error (for example the target of a mismatched
/// assignment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeError {
    kind: TypeErrorKind,
    span: Span,
    /// The type or count a construct required (rendered), where applicable.
    expected: Option<String>,
    /// The type(s) or count actually present (rendered), where applicable.
    actual: Option<String>,
    /// The offending operator symbol, for operator errors.
    operator: Option<String>,
    /// A related location involved in the error, where applicable.
    related: Option<Span>,
}

impl TypeError {
    /// Creates an expected/found mismatch error at `span` (`E-T01`).
    ///
    /// `expected` and `actual` are rendered type names (e.g. `Int`); `related`
    /// may point at the second location involved (e.g. the assignment
    /// target).
    pub fn mismatch(
        span: Span,
        expected: impl Into<String>,
        actual: impl Into<String>,
        related: Option<Span>,
    ) -> Self {
        Self {
            kind: TypeErrorKind::TypeMismatch,
            span,
            expected: Some(expected.into()),
            actual: Some(actual.into()),
            operator: None,
            related,
        }
    }

    /// Creates an invalid-operator error at `span` (`E-T02`).
    ///
    /// `actual` is the full operand phrase, e.g. `types `Int` and `Float``
    /// for a binary operator or `type `Bool`` for a unary one.
    pub fn invalid_operator(span: Span, operator: &str, actual: impl Into<String>) -> Self {
        Self {
            kind: TypeErrorKind::InvalidOperator,
            span,
            expected: None,
            actual: Some(actual.into()),
            operator: Some(operator.to_string()),
            related: None,
        }
    }

    /// Creates an invalid-range error at `span` (`E-T03`).
    ///
    /// `actual` renders the endpoint types, e.g. `` `Int` and `Float` ``.
    pub fn invalid_range(span: Span, actual: impl Into<String>) -> Self {
        Self {
            kind: TypeErrorKind::InvalidRange,
            span,
            expected: None,
            actual: Some(actual.into()),
            operator: None,
            related: None,
        }
    }

    /// Creates a not-callable error at `span` (`E-T04`), where `actual` is
    /// the rendered type of the called value.
    pub fn not_callable(span: Span, actual: impl Into<String>) -> Self {
        Self {
            kind: TypeErrorKind::NotCallable,
            span,
            expected: None,
            actual: Some(actual.into()),
            operator: None,
            related: None,
        }
    }

    /// Creates a wrong-argument-count error at `span` (`E-T05`).
    pub fn wrong_arg_count(span: Span, expected: usize, actual: usize) -> Self {
        Self {
            kind: TypeErrorKind::WrongArgumentCount,
            span,
            expected: Some(expected.to_string()),
            actual: Some(actual.to_string()),
            operator: None,
            related: None,
        }
    }

    /// Creates a not-iterable error at `span` (`E-T06`), where `actual` is
    /// the rendered type of the iterated value.
    pub fn not_iterable(span: Span, actual: impl Into<String>) -> Self {
        Self {
            kind: TypeErrorKind::NotIterable,
            span,
            expected: None,
            actual: Some(actual.into()),
            operator: None,
            related: None,
        }
    }

    /// Creates a custom-kind error at `span` with a rendered `detail` and
    /// an optional related span. Used by the aggregate rules (`E-T07`…
    /// `E-T18`), whose messages carry the offending names, types, and
    /// counts directly.
    fn custom(
        kind: TypeErrorKind,
        span: Span,
        detail: impl Into<String>,
        related: Option<Span>,
    ) -> Self {
        Self {
            kind,
            span,
            expected: None,
            actual: Some(detail.into()),
            operator: None,
            related,
        }
    }

    /// Creates a member-access-on-non-struct error at `span` (`E-T07`).
    pub fn member_access_on_non_struct(
        span: Span,
        member: &str,
        actual: impl Into<String>,
    ) -> Self {
        Self::custom(
            TypeErrorKind::MemberAccessOnNonStruct,
            span,
            format!(
                "cannot access member `{member}` of a value of type `{}`",
                actual.into()
            ),
            None,
        )
    }

    /// Creates an unknown-member error at `span` (`E-T08`).
    pub fn unknown_member(span: Span, struct_name: &str, member: &str) -> Self {
        Self::custom(
            TypeErrorKind::UnknownMember,
            span,
            format!("struct `{struct_name}` has no field named `{member}`"),
            None,
        )
    }

    /// Creates an index-on-non-array error at `span` (`E-T09`).
    pub fn index_on_non_array(span: Span, actual: impl Into<String>) -> Self {
        Self::custom(
            TypeErrorKind::IndexOnNonArray,
            span,
            format!("cannot index a value of type `{}`", actual.into()),
            None,
        )
    }

    /// Creates an invalid-index-type error at `span` (`E-T10`).
    pub fn invalid_index_type(span: Span, actual: impl Into<String>) -> Self {
        Self::custom(
            TypeErrorKind::InvalidIndexType,
            span,
            format!("an array index must be an `Int`, found `{}`", actual.into()),
            None,
        )
    }

    /// Creates an out-of-range constant index error at `span` (`E-T11`).
    pub fn index_out_of_range(span: Span, index: i64, len: u64) -> Self {
        Self::custom(
            TypeErrorKind::IndexOutOfRange,
            span,
            format!("array index `{index}` is out of bounds for an array of length {len}"),
            None,
        )
    }

    /// Creates an unknown-struct-field error at `span` (`E-T12`).
    pub fn unknown_struct_field(span: Span, struct_name: &str, field: &str) -> Self {
        Self::custom(
            TypeErrorKind::UnknownStructField,
            span,
            format!("struct `{struct_name}` has no field named `{field}`"),
            None,
        )
    }

    /// Creates a missing-struct-field error at `span` (`E-T13`).
    pub fn missing_struct_field(span: Span, struct_name: &str, field: &str) -> Self {
        Self::custom(
            TypeErrorKind::MissingStructField,
            span,
            format!("struct literal for `{struct_name}` is missing the field `{field}`"),
            None,
        )
    }

    /// Creates a duplicate-field-initializer error at `span` (`E-T14`),
    /// pointing at the first initializer of the same field as the related
    /// location.
    pub fn duplicate_field_init(span: Span, field: &str, first: Span) -> Self {
        Self::custom(
            TypeErrorKind::DuplicateFieldInit,
            span,
            format!("field `{field}` is initialized more than once"),
            Some(first),
        )
    }

    /// Creates an unknown-type error at `span` (`E-T15`).
    pub fn unknown_type(span: Span, name: &str) -> Self {
        Self::custom(
            TypeErrorKind::UnknownType,
            span,
            format!("cannot find type `{name}` in this scope"),
            None,
        )
    }

    /// Creates an invalid-array-length error at `span` (`E-T16`).
    pub fn invalid_array_length(span: Span, detail: impl Into<String>) -> Self {
        Self::custom(
            TypeErrorKind::InvalidArrayLength,
            span,
            format!("invalid array length: {}", detail.into()),
            None,
        )
    }

    /// Creates an empty-array-literal error at `span` (`E-T17`).
    pub fn empty_array_literal(span: Span) -> Self {
        Self::custom(
            TypeErrorKind::EmptyArrayLiteral,
            span,
            "cannot infer the element type of an empty array literal".to_string(),
            None,
        )
    }

    /// Creates an invalid-aggregate-layout error at `span` (`E-T18`).
    pub fn invalid_aggregate_layout(span: Span, detail: impl Into<String>) -> Self {
        Self::custom(
            TypeErrorKind::InvalidAggregateLayout,
            span,
            detail.into(),
            None,
        )
    }

    /// Creates an invalid-borrow-target error at `span` (`E-T19`): the
    /// borrowed expression is not a borrowable place, is already a
    /// reference, or is a deref-rooted place (reborrowing is deferred).
    pub fn invalid_borrow_target(span: Span, detail: impl Into<String>) -> Self {
        Self::custom(
            TypeErrorKind::InvalidBorrowTarget,
            span,
            detail.into(),
            None,
        )
    }

    /// Creates a deref-of-non-reference error at `span` (`E-T20`).
    pub fn deref_non_reference(span: Span, actual: impl Into<String>) -> Self {
        Self::custom(
            TypeErrorKind::DerefNonReference,
            span,
            format!("cannot dereference a value of type `{}`", actual.into()),
            None,
        )
    }

    /// Creates an assignment-through-immutable-reference error at `span`
    /// (`E-T21`).
    pub fn assign_through_immutable_ref(span: Span) -> Self {
        Self::custom(
            TypeErrorKind::AssignThroughImmutableRef,
            span,
            "cannot assign through an immutable reference; use `&mut`".to_string(),
            None,
        )
    }

    /// Creates a not-an-enum error at `span` (`E-T22`): the first path
    /// segment names a type that is not an enum.
    pub fn not_an_enum(span: Span, name: &str, actual: impl Into<String>) -> Self {
        Self::custom(
            TypeErrorKind::NotAnEnum,
            span,
            format!(
                "`{name}` is not an enum type (it is a `{}`); cannot access a variant of it",
                actual.into()
            ),
            None,
        )
    }

    /// Creates an unknown-variant error at `span` (`E-T23`).
    pub fn unknown_variant(span: Span, enum_name: &str, variant: &str) -> Self {
        Self::custom(
            TypeErrorKind::UnknownVariant,
            span,
            format!("enum `{enum_name}` has no variant named `{variant}`"),
            None,
        )
    }

    /// Creates a non-exhaustive-match error at `span` (`E-T24`), whose
    /// message explains what is missing (the uncovered variants, or the
    /// required catch-all arm).
    pub fn non_exhaustive_match(span: Span, detail: impl Into<String>) -> Self {
        Self::custom(TypeErrorKind::NonExhaustiveMatch, span, detail.into(), None)
    }

    /// Creates an unreachable-match-arm error at `span` (`E-T25`).
    pub fn unreachable_match_arm(span: Span, detail: impl Into<String>) -> Self {
        Self::custom(
            TypeErrorKind::UnreachableMatchArm,
            span,
            detail.into(),
            None,
        )
    }

    /// Creates an invalid-match-scrutinee error at `span` (`E-T26`), where
    /// `actual` is the rendered type of the matched value.
    pub fn invalid_match_scrutinee(span: Span, actual: impl Into<String>) -> Self {
        Self::custom(
            TypeErrorKind::InvalidMatchScrutinee,
            span,
            format!(
                "cannot match on a value of type `{}`; only `Int`, `Bool`, and enums are matchable",
                actual.into()
            ),
            None,
        )
    }

    /// Creates an invalid-variant-payload error at `span` (`E-T27`): the
    /// declared payload type is not a supported value type.
    pub fn invalid_variant_payload(span: Span, actual: impl Into<String>) -> Self {
        Self::custom(
            TypeErrorKind::InvalidVariantPayload,
            span,
            format!(
                "`{}` is not a supported variant payload type; a payload must be a value type with a deterministic layout",
                actual.into()
            ),
            None,
        )
    }

    /// Creates a variant-payload-mismatch error at `span` (`E-T28`): the
    /// constructed payload's type does not match the variant's declared
    /// payload type.
    pub fn variant_payload_mismatch(
        span: Span,
        expected: impl Into<String>,
        actual: impl Into<String>,
    ) -> Self {
        Self {
            kind: TypeErrorKind::VariantPayloadMismatch,
            span,
            expected: Some(expected.into()),
            actual: Some(actual.into()),
            operator: None,
            related: None,
        }
    }

    /// Creates a variant-payload-arity error at `span` (`E-T29`): a unit
    /// variant is constructed or matched with a payload, or a data-carrying
    /// variant without one.
    pub fn variant_payload_arity(span: Span, detail: impl Into<String>) -> Self {
        Self::custom(
            TypeErrorKind::VariantPayloadArity,
            span,
            detail.into(),
            None,
        )
    }

    /// Creates an enum-equality error at `span` (`E-T30`).
    pub fn enum_equality(span: Span, enum_name: impl Into<String>) -> Self {
        Self::custom(
            TypeErrorKind::EnumEquality,
            span,
            format!(
                "cannot compare values of enum type `{}` with `==`/`!=`; enum equality is not supported",
                enum_name.into()
            ),
            None,
        )
    }

    /// Creates an invalid-or-pattern error at `span` (`E-T34`): the
    /// alternatives of an or-pattern must bind exactly the same names, so
    /// the arm binds one value per name. `detail` names the offending
    /// name(s).
    pub fn invalid_or_pattern(span: Span, detail: impl Into<String>) -> Self {
        Self::custom(TypeErrorKind::InvalidOrPattern, span, detail.into(), None)
    }

    /// Creates a duplicate-discriminant error at `span` (`E-T31`): two
    /// variants of one enum share the same discriminant value, so the tag
    /// word cannot distinguish them. `first` points at the earlier variant
    /// that already uses the value.
    pub fn duplicate_discriminant(
        span: Span,
        enum_name: &str,
        variant: &str,
        value: i64,
        first: Span,
    ) -> Self {
        Self::custom(
            TypeErrorKind::DuplicateDiscriminant,
            span,
            format!(
                "variant `{variant}` of enum `{enum_name}` repeats the discriminant `{value}` used by an earlier variant; discriminants must be unique"
            ),
            Some(first),
        )
    }

    /// Creates a discriminant-overflow error at `span` (`E-T32`): a
    /// variant with no explicit discriminant follows a variant whose value
    /// is already the largest 64-bit integer, so the implicit continuation
    /// would overflow the tag word.
    pub fn discriminant_overflow(span: Span, enum_name: &str, variant: &str) -> Self {
        Self::custom(
            TypeErrorKind::DiscriminantOverflow,
            span,
            format!(
                "variant `{variant}` of enum `{enum_name}` needs an implicit discriminant, but the previous discriminant is already the largest 64-bit value; add an explicit discriminant"
            ),
            None,
        )
    }

    /// Creates a deref-rooted-assignment error at `span` (`E-T33`): the
    /// assignment target is a member or element of a dereferenced
    /// reference (`(*r).x = v`), which the reference model does not
    /// support (only whole-value `*r = v`). Rejected so the write is never
    /// silently dropped on a temporary copy.
    pub fn deref_rooted_assignment(span: Span) -> Self {
        Self::custom(
            TypeErrorKind::DerefRootedAssignment,
            span,
            "cannot assign through a member or element of a dereferenced reference; assign through the reference to the whole value (`*r = ...`)".to_string(),
            None,
        )
    }

    /// Creates a tuple-index-out-of-range error at `span` (`E-T35`): a
    /// tuple field access uses an index that is out of range.
    pub fn invalid_tuple_index(span: Span, index: u32, len: usize) -> Self {
        Self::custom(
            TypeErrorKind::TupleIndexOutOfRange,
            span,
            format!(
                "tuple index `{index}` is out of range; the tuple has {len} element{}",
                if len == 1 { "" } else { "s" }
            ),
            None,
        )
    }

    /// Creates a break-value-expected error at `span` (`E-T36`): `break;`
    /// inside a loop expression must carry a value.
    pub fn break_value_expected(span: Span) -> Self {
        Self::custom(
            TypeErrorKind::BreakValueExpected,
            span,
            "`break` inside a loop expression must carry a value (use `break expr;`)".to_string(),
            None,
        )
    }

    /// Creates a cannot-destructure error at `span` (`E-T37`): a tuple
    /// destructuring pattern is applied to a non-tuple type.
    pub fn cannot_destructure(span: Span, ty: &str) -> Self {
        Self::custom(
            TypeErrorKind::CannotDestructure,
            span,
            format!("cannot destructure `{ty}` (not a tuple type)"),
            None,
        )
    }

    /// Creates a destructure-arity-mismatch error at `span` (`E-T38`):
    /// the destructuring pattern has a different number of elements than
    /// the tuple type.
    pub fn destructure_arity_mismatch(span: Span, expected: usize, actual: usize) -> Self {
        Self::custom(
            TypeErrorKind::DestructureArityMismatch,
            span,
            format!(
                "tuple destructuring pattern has {actual} element{}, \
                 but the tuple type has {expected}",
                if actual == 1 { "" } else { "s" },
            ),
            None,
        )
    }

    /// The category of this error.
    pub fn kind(&self) -> TypeErrorKind {
        self.kind
    }

    /// The stable machine-readable code of this error (e.g. `E-T01`).
    pub fn code(&self) -> &'static str {
        self.kind.code()
    }

    /// The source span this error applies to.
    pub fn span(&self) -> Span {
        self.span
    }

    /// The type or count a construct required (rendered), when applicable.
    pub fn expected(&self) -> Option<&str> {
        self.expected.as_deref()
    }

    /// The type(s) or count actually present (rendered), when applicable.
    pub fn actual(&self) -> Option<&str> {
        self.actual.as_deref()
    }

    /// The offending operator symbol, for operator errors.
    pub fn operator(&self) -> Option<&str> {
        self.operator.as_deref()
    }

    /// A related location involved in the error, when applicable.
    pub fn related(&self) -> Option<Span> {
        self.related
    }
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let expected = self.expected.as_deref().unwrap_or("");
        let actual = self.actual.as_deref().unwrap_or("");
        let message = match self.kind {
            TypeErrorKind::TypeMismatch => format!("expected `{expected}`, found `{actual}`"),
            TypeErrorKind::InvalidOperator => format!(
                "cannot apply operator `{}` to {actual}",
                self.operator.as_deref().unwrap_or("")
            ),
            TypeErrorKind::InvalidRange => {
                format!("cannot construct a range with operands of types {actual}")
            }
            TypeErrorKind::NotCallable => format!("cannot call a value of type `{actual}`"),
            TypeErrorKind::WrongArgumentCount => {
                format!("expected `{expected}` arguments, found `{actual}`")
            }
            TypeErrorKind::VariantPayloadMismatch => {
                format!("expected a payload of type `{expected}`, found `{actual}`")
            }
            TypeErrorKind::NotIterable => format!("cannot iterate over a value of type `{actual}`"),
            // Aggregate diagnostics carry their full message in `actual`.
            TypeErrorKind::MemberAccessOnNonStruct
            | TypeErrorKind::UnknownMember
            | TypeErrorKind::IndexOnNonArray
            | TypeErrorKind::InvalidIndexType
            | TypeErrorKind::IndexOutOfRange
            | TypeErrorKind::UnknownStructField
            | TypeErrorKind::MissingStructField
            | TypeErrorKind::DuplicateFieldInit
            | TypeErrorKind::UnknownType
            | TypeErrorKind::InvalidArrayLength
            | TypeErrorKind::EmptyArrayLiteral
            | TypeErrorKind::InvalidAggregateLayout
            | TypeErrorKind::InvalidBorrowTarget
            | TypeErrorKind::DerefNonReference
            | TypeErrorKind::AssignThroughImmutableRef
            | TypeErrorKind::NotAnEnum
            | TypeErrorKind::UnknownVariant
            | TypeErrorKind::NonExhaustiveMatch
            | TypeErrorKind::UnreachableMatchArm
            | TypeErrorKind::InvalidMatchScrutinee
            | TypeErrorKind::InvalidVariantPayload
            | TypeErrorKind::VariantPayloadArity
            | TypeErrorKind::EnumEquality
            | TypeErrorKind::InvalidOrPattern
            | TypeErrorKind::DuplicateDiscriminant
            | TypeErrorKind::DiscriminantOverflow
            | TypeErrorKind::DerefRootedAssignment
            | TypeErrorKind::TupleIndexOutOfRange
            | TypeErrorKind::BreakValueExpected
            | TypeErrorKind::CannotDestructure
            | TypeErrorKind::DestructureArityMismatch => actual.to_string(),
        };
        f.write_str(&message)
    }
}
