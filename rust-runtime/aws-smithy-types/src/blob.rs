#[cfg(all(feature = "unstable", feature = "serialize"))]
use serde::Serialize;
#[cfg(all(feature = "unstable", feature = "deserialize"))]
use serde::{de::Visitor, Deserialize};

/// Binary Blob Type
///
/// Blobs represent protocol-agnostic binary content.
#[derive(Debug, PartialEq, Clone)]
pub struct Blob {
    inner: Vec<u8>,
}

#[cfg(all(feature = "unstable", feature = "serialize"))]
impl Serialize for Blob {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if serializer.is_human_readable() {
            serializer.serialize_str(&crate::base64::encode(&self.inner))
        } else {
            serializer.serialize_bytes(&self.inner)
        }
    }
}

#[cfg(all(feature = "unstable", feature = "deserialize"))]
struct HumanReadableBlobVisitor;

#[cfg(all(feature = "unstable", feature = "deserialize"))]
impl<'de> Visitor<'de> for HumanReadableBlobVisitor {
    type Value = Blob;
    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("expected base64 encoded string")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        match base64::decode(v) {
            Ok(inner) => Ok(Blob { inner }),
            Err(e) => Err(serde::de::Error::custom(e)),
        }
    }
}

#[cfg(all(feature = "unstable", feature = "deserialize"))]
struct NotHumanReadableBlobVisitor;

#[cfg(all(feature = "unstable", feature = "deserialize"))]
impl<'de> Visitor<'de> for NotHumanReadableBlobVisitor {
    type Value = Blob;
    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("expected base64 encoded string")
    }

    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Blob { inner: v })
    }
}

#[cfg(all(feature = "unstable", feature = "deserialize"))]
impl<'de> Deserialize<'de> for Blob {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            deserializer.deserialize_str(HumanReadableBlobVisitor)
        } else {
            deserializer.deserialize_byte_buf(NotHumanReadableBlobVisitor)
        }
    }
}

impl Blob {
    /// Creates a new blob from the given `input`.
    pub fn new<T: Into<Vec<u8>>>(input: T) -> Self {
        Blob {
            inner: input.into(),
        }
    }

    /// Consumes the `Blob` and returns a `Vec<u8>` with its contents.
    pub fn into_inner(self) -> Vec<u8> {
        self.inner
    }
}

impl AsRef<[u8]> for Blob {
    fn as_ref(&self) -> &[u8] {
        &self.inner
    }
}
