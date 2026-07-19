//! Hand-written V8 <-> Rust conversions for op arguments and return values.
//!
//! This replaces `serde_v8`, which is slated for deletion upstream and pinned us
//! to whatever `v8` release `deno_core` happened to be on.
//!
//! The conversions here deliberately reproduce `serde_v8`'s observable
//! behaviour rather than a cleaner design of our own: these ops are the
//! runtime's public API surface, so argument coercion rules, the objects handed
//! back to JS (including their null prototype) and the thrown error strings all
//! have to stay compatible.
//!
//! Convention: every deliberate quirk inherited from `serde_v8`, and every
//! deliberate divergence from it, carries a comment naming `serde_v8`. `grep
//! serde_v8 cli/src/run/ops/convert.rs` is therefore the complete inventory of
//! "we do this only because serde_v8 did," so each entry can be reviewed and
//! repaired on its own merits later. The name must appear in comments only --
//! never in a user-facing error string, since the crate is gone.

use v8;

/// Largest integer exactly representable as an f64, beyond which values are
/// handed to JS as BigInt instead of Number.
///
/// serde_v8 quirk: `u64`/`i64` return values silently change JS type at this
/// threshold, so a single op can hand back either a Number or a BigInt.
const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

/// Bound on nesting depth when converting arbitrary JS values into
/// `serde_json::Value`. Conversion recurses on the Rust stack, so without a
/// limit a deeply nested object built cheaply from user TypeScript would
/// overflow the stack and abort the process.
///
/// This applies to the `from_v8` direction only. `ToV8 for serde_json::Value`
/// recurses without any limit, as serde_v8's serializer also did; the only
/// producer of those values is `toml_parse`, and the `toml` parser rejects
/// over-deep input before we ever get here.
const RECURSION_LIMIT: usize = 128;

/// Cap on how many elements a sequence conversion may reserve up front. JS
/// array lengths are attacker-controlled and say nothing about how many
/// elements exist, so they are only ever used as an upper hint. Mirrors serde's
/// `size_hint::cautious` policy of bounding speculative preallocation.
const PREALLOC_LIMIT: usize = 4096;

/// Error strings intentionally match `serde_v8`'s wording (apart from the
/// `serde_v8 error: ` prefix, which was dropped now that the crate is gone) so
/// that existing error matching in user TypeScript mostly keeps working: these
/// messages are observable via the thrown `Error.message`.
#[derive(Debug, thiserror::Error)]
pub enum ConvertError {
    #[error("{0}")]
    Message(String),

    #[error("invalid type; expected: number, got: {0}")]
    ExpectedNumber(&'static str),

    #[error("invalid type; expected: string, got: {0}")]
    ExpectedString(&'static str),

    #[error("invalid type; expected: array, got: {0}")]
    ExpectedArray(&'static str),

    #[error("invalid type; expected: object, got: {0}")]
    ExpectedObject(&'static str),

    #[error("invalid type; expected: buffer, got: {0}")]
    ExpectedBuffer(&'static str),

    #[error("unsupported type")]
    UnsupportedType,

    #[error("recursion limit exceeded")]
    RecursionLimitExceeded,

    #[error("exception during value conversion")]
    V8Exception,

    #[error("can't create slice from resizable ArrayBuffer")]
    ResizableBackingStoreNotSupported,
}

pub type ConvertResult<T> = Result<T, ConvertError>;

/// Converts a JS value into a Rust value, as op arguments require.
pub trait FromV8: Sized {
    fn from_v8<'s>(
        scope: &mut v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::Value>,
    ) -> ConvertResult<Self>;
}

/// Converts a Rust value into a JS value, as op return values require.
///
/// Infallible by design: the only failure `serde_v8` could report here was an
/// over-long string, and the sole caller unwrapped the result anyway.
pub trait ToV8 {
    fn to_v8<'s>(self, scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value>;
}

/// Supplies the value for a struct field that was absent (or `undefined`) on
/// the incoming object. Mirrors serde's derived behaviour as used by serde_v8:
/// `Option` fields default to `None`, everything else is a hard error.
pub trait Field: FromV8 {
    fn missing(name: &'static str) -> ConvertResult<Self> {
        Err(ConvertError::Message(format!("missing field `{}`", name)))
    }
}

// Owned copy of a JS `ArrayBuffer`/`ArrayBufferView`'s bytes, used for op
// arguments.
//
// Deliberate divergence from serde_v8: it aliased the backing store, which was
// unsound because JS could mutate or detach the buffer while Rust held a slice
// into it. Copying costs one memcpy and removes that class of bug entirely.
pub struct JsBuffer(Vec<u8>);

impl std::ops::Deref for JsBuffer {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8]> for JsBuffer {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Bytes handed back to JS as a `Uint8Array`.
pub struct ToJsBuffer(Vec<u8>);

impl From<Vec<u8>> for ToJsBuffer {
    fn from(bytes: Vec<u8>) -> Self {
        ToJsBuffer(bytes)
    }
}

/// Builds the null-prototype object `serde_v8` produced for every struct and
/// map it serialized.
///
/// serde_v8 quirk: values returned from ops therefore have no `Object.prototype`
/// -- no `hasOwnProperty`, no `toString`, and `instanceof Object` is false.
/// Kept because user TypeScript may already depend on it.
pub fn null_proto_object<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    keys: &[v8::Local<v8::Name>],
    values: &[v8::Local<v8::Value>],
) -> v8::Local<'s, v8::Value> {
    let null = v8::null(scope).into();
    v8::Object::with_prototype_and_properties(scope, null, keys, values).into()
}

pub fn expect_object<'s>(
    value: v8::Local<'s, v8::Value>,
) -> ConvertResult<v8::Local<'s, v8::Object>> {
    v8::Local::<v8::Object>::try_from(value)
        .map_err(|_| ConvertError::ExpectedObject(value.type_repr()))
}

/// Reads one struct field. `Ok(None)` means the property was absent or
/// `undefined`; deciding whether that is fatal is left to [`Field::missing`].
///
/// serde_v8 quirk: this reproduces serde's two-phase derive, where every
/// present field is type-checked before any missing field is reported. So an
/// object with both a missing field and a mistyped one reports the type error,
/// not the missing one. Note this ordering is only maintained by hand -- the
/// macros below do not enforce that fields are listed in declaration order, and
/// getting the order wrong silently changes which error a user sees.
pub fn read_field<'s, T: FromV8>(
    scope: &mut v8::PinScope<'s, '_>,
    obj: v8::Local<'s, v8::Object>,
    name: &'static str,
) -> ConvertResult<Option<T>> {
    let key = v8::String::new(scope, name).ok_or(ConvertError::V8Exception)?;
    let value = obj
        .get(scope, key.into())
        .ok_or(ConvertError::V8Exception)?;
    if value.is_undefined() {
        return Ok(None);
    }
    T::from_v8(scope, value).map(Some)
}

/// Implements [`FromV8`] for a struct by reading each named property off the
/// incoming object. Fields must be listed in declaration order so that type
/// errors surface in the same order serde_v8's serde derive reported them; this
/// is a caller obligation, unenforced by the macro.
#[macro_export]
macro_rules! impl_from_v8_struct {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        impl $crate::run::ops::convert::FromV8 for $name {
            fn from_v8<'s>(
                scope: &mut v8::PinScope<'s, '_>,
                value: v8::Local<'s, v8::Value>,
            ) -> $crate::run::ops::convert::ConvertResult<Self> {
                let obj = $crate::run::ops::convert::expect_object(value)?;
                $(
                    let $field =
                        $crate::run::ops::convert::read_field::<$ty>(
                            scope,
                            obj,
                            stringify!($field),
                        )?;
                )*
                Ok(Self {
                    $($field: match $field {
                        Some(v) => v,
                        None => <$ty as $crate::run::ops::convert::Field>::missing(
                            stringify!($field),
                        )?,
                    },)*
                })
            }
        }
    };
}

/// Implements [`FromV8`] for a fieldless struct.
///
/// serde_v8 quirk: matches serde's unit-struct handling, which accepts any
/// input at all -- a string, a number, `null` -- without inspecting it.
#[macro_export]
macro_rules! impl_from_v8_unit_struct {
    ($name:ident) => {
        impl $crate::run::ops::convert::FromV8 for $name {
            fn from_v8<'s>(
                _scope: &mut v8::PinScope<'s, '_>,
                _value: v8::Local<'s, v8::Value>,
            ) -> $crate::run::ops::convert::ConvertResult<Self> {
                Ok($name)
            }
        }
    };
}

/// Implements [`ToV8`] for a struct, emitting the fields as own properties in
/// declaration order.
#[macro_export]
macro_rules! impl_to_v8_struct {
    ($name:ident { $($field:ident),* $(,)? }) => {
        impl $crate::run::ops::convert::ToV8 for $name {
            fn to_v8<'s>(
                self,
                scope: &mut v8::PinScope<'s, '_>,
            ) -> v8::Local<'s, v8::Value> {
                let keys: [v8::Local<v8::Name>; [$(stringify!($field)),*].len()] = [
                    $(v8::String::new(scope, stringify!($field)).unwrap().into(),)*
                ];
                let values: [v8::Local<v8::Value>; [$(stringify!($field)),*].len()] = [
                    $($crate::run::ops::convert::ToV8::to_v8(self.$field, scope),)*
                ];
                $crate::run::ops::convert::null_proto_object(scope, &keys, &values)
            }
        }
    };
}

// --- FromV8 for primitives -------------------------------------------------

// serde_v8 quirk: an argument typed `()` accepts anything at all, including a
// missing or wrongly-typed value, because serde's unit visitor never inspects
// the input.
impl FromV8 for () {
    fn from_v8<'s>(
        _scope: &mut v8::PinScope<'s, '_>,
        _value: v8::Local<'s, v8::Value>,
    ) -> ConvertResult<Self> {
        Ok(())
    }
}

impl FromV8 for String {
    fn from_v8<'s>(
        scope: &mut v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::Value>,
    ) -> ConvertResult<Self> {
        // serde_v8 quirk: boxed `String` objects (`new String("x")`) are
        // accepted alongside primitive strings, unlike most JS APIs.
        if value.is_string() || value.is_string_object() {
            Ok(value.to_rust_string_lossy(scope))
        } else {
            Err(ConvertError::ExpectedString(value.type_repr()))
        }
    }
}

impl FromV8 for f64 {
    fn from_v8<'s>(
        _scope: &mut v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::Value>,
    ) -> ConvertResult<Self> {
        // serde_v8 quirk: a BigInt argument is accepted where a number is
        // expected and lossily narrowed to f64, rather than being rejected.
        if let Ok(n) = v8::Local::<v8::Number>::try_from(value) {
            Ok(n.value())
        } else if let Ok(n) = v8::Local::<v8::BigInt>::try_from(value) {
            Ok(bigint_to_f64(n))
        } else {
            Err(ConvertError::ExpectedNumber(value.type_repr()))
        }
    }
}

impl<T: FromV8> FromV8 for Option<T> {
    fn from_v8<'s>(
        scope: &mut v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::Value>,
    ) -> ConvertResult<Self> {
        if value.is_null_or_undefined() {
            Ok(None)
        } else {
            T::from_v8(scope, value).map(Some)
        }
    }
}

impl<T: FromV8> FromV8 for Vec<T> {
    fn from_v8<'s>(
        scope: &mut v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::Value>,
    ) -> ConvertResult<Self> {
        let array = v8::Local::<v8::Array>::try_from(value)
            .map_err(|_| ConvertError::ExpectedArray(value.type_repr()))?;
        let len = array.length();
        // NOT `Vec::with_capacity(len)`: a JS array's `length` is settable
        // independently of its contents (`a.length = 4294967295`), so it is an
        // attacker-controlled number that says nothing about how much data
        // actually exists. Preallocating from it is an abort-the-process memory
        // bomb reachable before any capability check. Grow as elements arrive
        // instead, capping the initial reservation the way serde does.
        //
        // Deliberate divergence from serde_v8: neither serde (which caps
        // speculative reservations at 1 MiB) nor serde_json's `Value` visitor
        // (plain `Vec::new()`) preallocated from an untrusted length either, so
        // this restores the pre-serde_v8-removal behaviour rather than changing
        // it.
        let mut out = Vec::with_capacity((len as usize).min(PREALLOC_LIMIT));
        for i in 0..len {
            let element = array.get_index(scope, i).ok_or(ConvertError::V8Exception)?;
            out.push(T::from_v8(scope, element)?);
        }
        Ok(out)
    }
}

impl FromV8 for JsBuffer {
    fn from_v8<'s>(
        scope: &mut v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::Value>,
    ) -> ConvertResult<Self> {
        if let Ok(view) = v8::Local::<v8::ArrayBufferView>::try_from(value) {
            let buffer = view
                .buffer(scope)
                .ok_or(ConvertError::ExpectedBuffer(value.type_repr()))?;
            check_backing_store(buffer, value)?;
            let mut bytes = vec![0u8; view.byte_length()];
            view.copy_contents(&mut bytes);
            Ok(JsBuffer(bytes))
        } else if let Ok(buffer) = v8::Local::<v8::ArrayBuffer>::try_from(value) {
            check_backing_store(buffer, value)?;
            let len = buffer.byte_length();
            let view = v8::Uint8Array::new(scope, buffer, 0, len)
                .ok_or(ConvertError::ExpectedBuffer(value.type_repr()))?;
            let mut bytes = vec![0u8; len];
            view.copy_contents(&mut bytes);
            Ok(JsBuffer(bytes))
        } else {
            Err(ConvertError::ExpectedBuffer(value.type_repr()))
        }
    }
}

fn check_backing_store(
    buffer: v8::Local<v8::ArrayBuffer>,
    value: v8::Local<v8::Value>,
) -> ConvertResult<()> {
    let store = buffer.get_backing_store();
    if store.is_resizable_by_user_javascript() {
        Err(ConvertError::ResizableBackingStoreNotSupported)
    } else if store.is_shared() {
        Err(ConvertError::ExpectedBuffer(value.type_repr()))
    } else {
        Ok(())
    }
}

impl FromV8 for serde_json::Value {
    fn from_v8<'s>(
        scope: &mut v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::Value>,
    ) -> ConvertResult<Self> {
        json_from_v8(scope, value, RECURSION_LIMIT)
    }
}

fn json_from_v8<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'s, v8::Value>,
    depth: usize,
) -> ConvertResult<serde_json::Value> {
    // Discrimination order matters: an ArrayBuffer is also an object, and an
    // Array is also an object.
    if value.is_boolean() {
        Ok(serde_json::Value::Bool(value.is_true()))
    } else if value.is_number() {
        Ok(json_number(value))
    } else if value.is_string() {
        Ok(serde_json::Value::String(value.to_rust_string_lossy(scope)))
    } else if value.is_array() {
        let depth = depth
            .checked_sub(1)
            .ok_or(ConvertError::RecursionLimitExceeded)?;
        let array = v8::Local::<v8::Array>::try_from(value)
            .map_err(|_| ConvertError::ExpectedArray(value.type_repr()))?;
        // See `FromV8 for Vec<T>`: `array.length()` is attacker-controlled and
        // unrelated to the number of elements actually present, so it must not
        // drive an allocation.
        let mut out = Vec::with_capacity((array.length() as usize).min(PREALLOC_LIMIT));
        for i in 0..array.length() {
            let element = array.get_index(scope, i).ok_or(ConvertError::V8Exception)?;
            out.push(json_from_v8(scope, element, depth)?);
        }
        Ok(serde_json::Value::Array(out))
    } else if value.is_big_int() {
        Err(ConvertError::UnsupportedType)
    } else if value.is_array_buffer() || value.is_array_buffer_view() {
        // serde_json's value visitor rejects byte input outright; reproduce the
        // message serde generated for it.
        Err(ConvertError::Message(
            "invalid type: byte array, expected any valid JSON value".to_string(),
        ))
    } else if value.is_object() {
        let depth = depth
            .checked_sub(1)
            .ok_or(ConvertError::RecursionLimitExceeded)?;
        let obj = expect_object(value)?;
        // serde_v8 quirk: `v8::Map` was special-cased ahead of the generic
        // own-property walk, so a `Map`'s entries -- which are internal slots,
        // not properties -- do become part of the resulting value. This diverges
        // from `JSON.stringify`, which yields `{}` for a `Map`. Without this
        // branch a `Map` falls through to `get_own_property_names`, which
        // returns `[]`, and the map silently converts to `{}`.
        //
        // serde_v8 quirk: a non-string key is an error, not a skipped or
        // coerced entry -- its `MapPairsAccess` deserialized each key through
        // the same string path as any other map key, so `new Map([[1, 2]])`
        // throws `invalid type; expected: string, got: Number`. Note also that,
        // unlike the object path below, an entry whose value is `undefined` is
        // kept (as `null`) rather than dropped, because that skipping lived in
        // serde_v8's object access and not in `MapPairsAccess`.
        if let Ok(map) = v8::Local::<v8::Map>::try_from(value) {
            // `Map::as_array` returns a flat [k0, v0, k1, v1, ...] array in
            // insertion order, which is how serde_v8 enumerated entries.
            let pairs = map.as_array(scope);
            let len = pairs.length();
            let mut out = serde_json::Map::new();
            let mut i = 0;
            while i + 1 < len {
                let key = pairs.get_index(scope, i).ok_or(ConvertError::V8Exception)?;
                let entry = pairs
                    .get_index(scope, i + 1)
                    .ok_or(ConvertError::V8Exception)?;
                let key = String::from_v8(scope, key)?;
                out.insert(key, json_from_v8(scope, entry, depth)?);
                i += 2;
            }
            return Ok(serde_json::Value::Object(out));
        }
        // serde_v8 quirk: only *own* properties become part of a
        // `serde_json::Value`, whereas struct fields are read with `obj.get()`
        // and so do walk the prototype chain. The two directions disagree.
        let names = obj
            .get_own_property_names(
                scope,
                v8::GetPropertyNamesArgsBuilder::new()
                    .key_conversion(v8::KeyConversionMode::ConvertToString)
                    .build(),
            )
            .ok_or(ConvertError::V8Exception)?;
        let mut map = serde_json::Map::new();
        for i in 0..names.length() {
            let key = names.get_index(scope, i).ok_or(ConvertError::V8Exception)?;
            let entry = obj.get(scope, key).ok_or(ConvertError::V8Exception)?;
            // serde_v8 quirk: keys whose value is `undefined` are dropped
            // entirely rather than becoming `null`.
            if entry.is_undefined() {
                continue;
            }
            let key = String::from_v8(scope, key)?;
            map.insert(key, json_from_v8(scope, entry, depth)?);
        }
        Ok(serde_json::Value::Object(map))
    } else if value.is_null_or_undefined() {
        Ok(serde_json::Value::Null)
    } else {
        // Deliberate divergence from serde_v8: it reached
        // `panic!("serde_v8: unknown ValueType")` here, so a Symbol argument
        // aborted the process. We return a catchable error instead.
        Err(ConvertError::UnsupportedType)
    }
}

fn json_number(value: v8::Local<v8::Value>) -> serde_json::Value {
    let number = v8::Local::<v8::Number>::try_from(value)
        .expect("caller checked is_number")
        .value();
    if value.is_uint32() {
        serde_json::Value::from(number as u32)
    } else if value.is_int32() {
        serde_json::Value::from(number as i32)
    } else {
        // Non-finite values have no JSON representation and become null, as
        // serde_json's `From<f64>` does.
        serde_json::Value::from(number)
    }
}

fn bigint_to_f64(value: v8::Local<v8::BigInt>) -> f64 {
    // log2(f64::MAX) == 1024, so 16 64-bit words cover the whole f64 range.
    let mut words: [u64; 16] = [0; 16];
    let (negative, words) = value.to_words_array(&mut words);
    if value.word_count() > 16 {
        return if negative {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    let sign = if negative { -1.0 } else { 1.0 };
    let magnitude: f64 = words
        .iter()
        .enumerate()
        .map(|(i, w)| (*w as f64) * 2.0f64.powi(64 * i as i32))
        .sum();
    sign * magnitude
}

// --- Field defaults --------------------------------------------------------

impl Field for String {}
impl Field for f64 {}
impl Field for JsBuffer {}
impl Field for serde_json::Value {}
impl<T: FromV8> Field for Vec<T> {}

impl<T: FromV8> Field for Option<T> {
    fn missing(_name: &'static str) -> ConvertResult<Self> {
        Ok(None)
    }
}

// --- ToV8 for primitives ---------------------------------------------------

impl ToV8 for () {
    fn to_v8<'s>(self, scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        // serde_v8 quirk: the unit type crosses to JS as `null`, not
        // `undefined`, so void ops resolve with `null`.
        v8::null(scope).into()
    }
}

impl ToV8 for bool {
    fn to_v8<'s>(self, scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        v8::Boolean::new(scope, self).into()
    }
}

impl ToV8 for u8 {
    fn to_v8<'s>(self, scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        u32::from(self).to_v8(scope)
    }
}

impl ToV8 for u32 {
    fn to_v8<'s>(self, scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        v8::Integer::new_from_unsigned(scope, self).into()
    }
}

impl ToV8 for i32 {
    fn to_v8<'s>(self, scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        v8::Integer::new(scope, self).into()
    }
}

impl ToV8 for u64 {
    fn to_v8<'s>(self, scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        if self <= MAX_SAFE_INTEGER {
            v8::Number::new(scope, self as f64).into()
        } else {
            v8::BigInt::new_from_u64(scope, self).into()
        }
    }
}

impl ToV8 for i64 {
    fn to_v8<'s>(self, scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        if self.unsigned_abs() <= MAX_SAFE_INTEGER {
            v8::Number::new(scope, self as f64).into()
        } else {
            v8::BigInt::new_from_i64(scope, self).into()
        }
    }
}

impl ToV8 for f64 {
    fn to_v8<'s>(self, scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        v8::Number::new(scope, self).into()
    }
}

impl ToV8 for String {
    fn to_v8<'s>(self, scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        // serde_v8 quirk: `v8::String::new` returns `None` past V8's ~512 MB
        // string limit. serde_v8 reported an error, but its sole call site
        // unwrapped it, so an over-long string aborted the process either way.
        // Preserved rather than silently changed; ToV8 is infallible by design.
        v8::String::new(scope, &self).unwrap().into()
    }
}

impl<T: ToV8> ToV8 for Option<T> {
    fn to_v8<'s>(self, scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        // serde_v8 quirk: `None` crosses to JS as `null`, not `undefined`, so an
        // absent optional field is still present as a key with value `null`.
        match self {
            Some(value) => value.to_v8(scope),
            None => v8::null(scope).into(),
        }
    }
}

impl<T: ToV8> ToV8 for Vec<T> {
    // serde_v8 quirk: this applies to `Vec<u8>` too, so `ExecResult.stdout` and
    // `.stderr` reach JS as an Array of numbers rather than a `Uint8Array`. The
    // published `.d.ts` claims `Uint8Array` and is simply wrong. Use
    // `ToJsBuffer` for anything that should be a real typed array.
    fn to_v8<'s>(self, scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        let elements: Vec<v8::Local<v8::Value>> =
            self.into_iter().map(|item| item.to_v8(scope)).collect();
        v8::Array::new_with_elements(scope, &elements).into()
    }
}

impl ToV8 for ToJsBuffer {
    fn to_v8<'s>(self, scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        let bytes = self.0.into_boxed_slice();
        if bytes.is_empty() {
            let buffer = v8::ArrayBuffer::new(scope, 0);
            return v8::Uint8Array::new(scope, buffer, 0, 0)
                .expect("Failed to create Uint8Array")
                .into();
        }
        let len = bytes.len();
        let backing_store =
            v8::ArrayBuffer::new_backing_store_from_boxed_slice(bytes).make_shared();
        let buffer = v8::ArrayBuffer::with_backing_store(scope, &backing_store);
        v8::Uint8Array::new(scope, buffer, 0, len)
            .expect("Failed to create Uint8Array")
            .into()
    }
}

impl ToV8 for serde_json::Value {
    // serde_v8 quirk: unlike `json_from_v8`, this direction has no recursion
    // limit, matching serde_v8's serializer. Safe today only because the sole
    // producer is `toml_parse` and the `toml` parser caps nesting first; adding
    // a producer of arbitrarily deep values would make this a stack overflow.
    fn to_v8<'s>(self, scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        match self {
            serde_json::Value::Null => v8::null(scope).into(),
            serde_json::Value::Bool(b) => b.to_v8(scope),
            serde_json::Value::Number(n) => {
                // Match serde_json's own Serialize impl, which dispatches on the
                // number's internal representation.
                if let Some(u) = n.as_u64() {
                    u.to_v8(scope)
                } else if let Some(i) = n.as_i64() {
                    i.to_v8(scope)
                } else {
                    n.as_f64().unwrap_or(f64::NAN).to_v8(scope)
                }
            }
            serde_json::Value::String(s) => s.to_v8(scope),
            serde_json::Value::Array(items) => items.to_v8(scope),
            serde_json::Value::Object(map) => {
                let mut keys: Vec<v8::Local<v8::Name>> = Vec::with_capacity(map.len());
                let mut values: Vec<v8::Local<v8::Value>> = Vec::with_capacity(map.len());
                for (key, value) in map {
                    keys.push(v8::String::new(scope, &key).unwrap().into());
                    values.push(value.to_v8(scope));
                }
                null_proto_object(scope, &keys, &values)
            }
        }
    }
}
