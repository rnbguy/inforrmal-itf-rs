use std::fmt;

use num_traits::ToPrimitive;
use serde::de::value::{MapDeserializer, SeqDeserializer};
use serde::de::{
    DeserializeOwned, DeserializeSeed, Deserializer, EnumAccess, Error as SerdeError, Expected,
    IntoDeserializer, Unexpected, VariantAccess, Visitor,
};
use serde::Deserialize;

use crate::bigint::BigInt;
use crate::map::Map;
use crate::set::Set;
use crate::tuple::Tuple;
use crate::value::{Type, Value};

pub fn decode_value<T>(value: Value) -> Result<T, Error>
where
    T: DeserializeOwned,
{
    T::deserialize(value)
}

#[derive(Debug)]
pub enum Error {
    Custom(String),
    TypeMismatch(Type, Type),
    BigInt(BigInt, &'static str),
    UnsupportedType(&'static str),
    Number(i64, &'static str),
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Custom(msg) => msg.fmt(f),

            Error::TypeMismatch(expected, actual) => {
                write!(f, "type mismatch: expected {expected:?}, found {actual:?}")
            }

            Error::BigInt(value, expected) => {
                write!(f, "cannot convert {value} to {expected}")
            }

            Error::Number(value, expected) => {
                write!(f, "cannot convert {value} to {expected}")
            }

            Error::UnsupportedType(ty) => write!(f, "unsupported type: {ty}"),
        }
    }
}

impl SerdeError for Error {
    fn custom<T>(msg: T) -> Self
    where
        T: fmt::Display,
    {
        Self::Custom(msg.to_string())
    }
}

impl Value {
    fn invalid_type<E>(&self, exp: &dyn Expected) -> E
    where
        E: serde::de::Error,
    {
        serde::de::Error::invalid_type(self.unexpected(), exp)
    }

    fn unexpected(&self) -> Unexpected {
        match self {
            Value::Bool(b) => Unexpected::Bool(*b),
            Value::Number(n) => Unexpected::Signed(*n),
            Value::String(s) => Unexpected::Str(s),
            Value::List(_) => Unexpected::Seq,
            Value::Map(_) => Unexpected::Map,
            Value::Record(_) => Unexpected::Other("record"),
            Value::BigInt(_) => Unexpected::Other("bigint"),
            Value::Tuple(_) => Unexpected::Other("tuple"),
            Value::Set(_) => Unexpected::Other("set"),
            Value::Unserializable(_) => Unexpected::Other("unserializable"),
        }
    }
}

macro_rules! deserialize_number {
    ($ty:ident, $visit:ident, $method:ident) => {
        fn $method<V>(self, visitor: V) -> Result<V::Value, Error>
        where
            V: Visitor<'de>,
        {
            match self {
                Value::Number(n) => visitor.$visit($ty::try_from(n).unwrap()),
                _ => Err(self.invalid_type(&visitor)),
            }
        }
    };
}

impl<'de> IntoDeserializer<'de, Error> for Value {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self::Deserializer {
        self
    }
}

impl<'de> Deserializer<'de> for Value {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Value::Bool(v) => visitor.visit_bool(v),
            Value::Number(v) => visitor.visit_i64(v),
            Value::String(v) => visitor.visit_string(v),
            Value::BigInt(v) => visitor.visit_i64(v.to_i64().unwrap()),
            Value::List(v) => visit_list(v, visitor),
            Value::Tuple(v) => visit_tuple(v, visitor),
            Value::Set(v) => visit_set(v, visitor),
            Value::Record(v) => visit_record(v, visitor),
            Value::Map(v) => visit_map(v, visitor),
            Value::Unserializable(_) => Err(Error::UnsupportedType("unserializable")),
        }
    }

    deserialize_number!(i8, visit_i8, deserialize_i8);
    deserialize_number!(i16, visit_i16, deserialize_i16);
    deserialize_number!(i32, visit_i32, deserialize_i32);
    deserialize_number!(i64, visit_i64, deserialize_i64);
    deserialize_number!(i128, visit_i128, deserialize_i128);
    deserialize_number!(u8, visit_u8, deserialize_u8);
    deserialize_number!(u16, visit_u16, deserialize_u16);
    deserialize_number!(u32, visit_u32, deserialize_u32);
    deserialize_number!(u64, visit_u64, deserialize_u64);
    deserialize_number!(u128, visit_u128, deserialize_u128);

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_some(self)
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Value::Bool(v) => visitor.visit_bool(v),
            _ => Err(self.invalid_type(&visitor)),
        }
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Value::String(v) => visitor.visit_string(v),
            _ => Err(self.invalid_type(&visitor)),
        }
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_byte_buf(visitor)
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Value::String(v) => visitor.visit_string(v),
            Value::List(v) => visit_list(v, visitor),
            _ => Err(self.invalid_type(&visitor)),
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        Err(self.invalid_type(&visitor))
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(self.invalid_type(&visitor))
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(self.invalid_type(&visitor))
    }

    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Value::List(v) => visit_list(v, visitor),
            Value::Tuple(v) => visit_tuple(v, visitor),
            _ => Err(self.invalid_type(&visitor)),
        }
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Value::Map(v) => visit_map(v, visitor),
            _ => Err(self.invalid_type(&visitor)),
        }
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Value::Record(v) => visit_record(v, visitor),
            _ => Err(self.invalid_type(&visitor)),
        }
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let (variant, value) = match self {
            Value::Record(value) => {
                let mut iter = value.into_iter();
                let (variant, value) = match iter.next() {
                    Some(v) => v,
                    None => {
                        return Err(serde::de::Error::invalid_value(
                            Unexpected::Map,
                            &"map with a single key",
                        ));
                    }
                };
                if iter.next().is_some() {
                    return Err(serde::de::Error::invalid_value(
                        Unexpected::Map,
                        &"map with a single key",
                    ));
                }
                (variant, Some(value))
            }
            Value::String(variant) => (variant, None),
            other => {
                return Err(serde::de::Error::invalid_type(
                    other.unexpected(),
                    &"string or map",
                ));
            }
        };

        visitor.visit_enum(EnumDeserializer { variant, value })
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        drop(self);
        visitor.visit_unit()
    }
}

fn visit_map<'de, V>(v: Map<Value, Value>, visitor: V) -> Result<V::Value, Error>
where
    V: Visitor<'de>,
{
    let mut deserializer = MapDeserializer::new(v.into_iter());
    let map = visitor.visit_map(&mut deserializer)?;
    Ok(map)
}

fn visit_record<'de, V>(record: Map<String, Value>, visitor: V) -> Result<V::Value, Error>
where
    V: Visitor<'de>,
{
    let mut deserializer = MapDeserializer::new(record.into_iter());
    let map = visitor.visit_map(&mut deserializer)?;
    Ok(map)
}

fn visit_set<'de, V>(v: Set<Value>, visitor: V) -> Result<V::Value, Error>
where
    V: Visitor<'de>,
{
    let mut deserializer = SeqDeserializer::new(v.into_iter());
    let seq = visitor.visit_seq(&mut deserializer)?;
    Ok(seq)
}

fn visit_tuple<'de, V>(v: Tuple<Value>, visitor: V) -> Result<V::Value, Error>
where
    V: Visitor<'de>,
{
    let mut deserializer = SeqDeserializer::new(v.into_iter());
    let seq = visitor.visit_seq(&mut deserializer)?;
    Ok(seq)
}

fn visit_list<'de, V>(v: Vec<Value>, visitor: V) -> Result<V::Value, Error>
where
    V: Visitor<'de>,
{
    let mut deserializer = SeqDeserializer::new(v.into_iter());
    let seq = visitor.visit_seq(&mut deserializer)?;
    Ok(seq)
}

struct EnumDeserializer {
    variant: String,
    value: Option<Value>,
}

impl<'de> EnumAccess<'de> for EnumDeserializer {
    type Error = Error;
    type Variant = VariantDeserializer;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Error>
    where
        V: DeserializeSeed<'de>,
    {
        let variant = self.variant.into_deserializer();
        let visitor = VariantDeserializer { value: self.value };
        seed.deserialize(variant).map(|v| (v, visitor))
    }
}

struct VariantDeserializer {
    value: Option<Value>,
}

impl<'de> VariantAccess<'de> for VariantDeserializer {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Error> {
        match self.value {
            Some(value) => Deserialize::deserialize(value),
            None => Ok(()),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.value {
            Some(value) => seed.deserialize(value),
            None => Err(serde::de::Error::invalid_type(
                Unexpected::UnitVariant,
                &"newtype variant",
            )),
        }
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            Some(Value::Tuple(v)) => {
                if v.is_empty() {
                    visitor.visit_unit()
                } else {
                    visit_tuple(v, visitor)
                }
            }
            // Some(Value::List(v)) => {
            //     if v.is_empty() {
            //         visitor.visit_unit()
            //     } else {
            //         visit_list(v, visitor)
            //     }
            // }
            Some(other) => Err(serde::de::Error::invalid_type(
                other.unexpected(),
                &"tuple variant",
            )),
            None => Err(serde::de::Error::invalid_type(
                Unexpected::UnitVariant,
                &"tuple variant",
            )),
        }
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            Some(Value::Record(v)) => visit_record(v, visitor),
            Some(other) => Err(serde::de::Error::invalid_type(
                other.unexpected(),
                &"struct variant",
            )),
            None => Err(serde::de::Error::invalid_type(
                Unexpected::UnitVariant,
                &"struct variant",
            )),
        }
    }
}
