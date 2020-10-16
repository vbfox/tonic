use std::{fmt::Debug, io};

use bytes::{Buf, BytesMut};
use http::HeaderValue;
use tracing::debug;

use crate::metadata::MetadataMap;

use super::{Compressor, ENCODING_HEADER, compressors::{self, IDENTITY}};

pub(crate) const BUFFER_SIZE: usize = 8 * 1024;
pub(crate) const ACCEPT_ENCODING_HEADER: &str = "grpc-accept-encoding";

#[derive(Clone)]
pub(crate) struct Compression {
    compressor: Option<&'static Box<dyn Compressor>>,
}

impl Debug for Compression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Compression")
            .field(
                "compressor",
                &self.compressor.map(|c| c.name()).unwrap_or(IDENTITY),
            )
            .finish()
    }
}

fn parse_accept_encoding_header(value: &str) -> Vec<&str> {
    value
        .split(",")
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>()
}

fn first_supported_compressor(accepted: &Vec<&str>) -> Option<&'static Box<dyn Compressor>> {
    accepted
        .iter()
        .filter(|name| **name != IDENTITY)
        .filter_map(|name| compressors::get(name))
        .next()
}

impl Compression {
    /// Create an instance of compression that doesn't compress anything
    pub(crate) fn disabled() -> Compression {
        Compression { compressor: None }
    }

    /// Create an instance of compression from GRPC metadata
    pub(crate) fn response_from_metadata(request_metadata: &MetadataMap) -> Compression {
        let accept_encoding_header = request_metadata
            .get(ACCEPT_ENCODING_HEADER)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let parsed = parse_accept_encoding_header(accept_encoding_header);
        let compressor = first_supported_compressor(&parsed);
        Compression { compressor }
    }

    /// Create an instance of compression from HTTP headers
    pub(crate) fn response_from_headers(request_headers: &http::HeaderMap) -> Compression {
        let accept_encoding_header = request_headers
            .get(ACCEPT_ENCODING_HEADER)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let parsed = parse_accept_encoding_header(accept_encoding_header);
        let compressor = first_supported_compressor(&parsed);
        Compression { compressor }
    }

    /// Get if compression is enabled
    pub(crate) fn is_enabled(&self) -> bool {
        self.compressor.is_some()
    }

    /// Decompress `len` bytes from `in_buffer` into `out_buffer`
    pub(crate) fn compress(
        &self,
        in_buffer: &mut BytesMut,
        out_buffer: &mut BytesMut,
        len: usize,
    ) -> Result<(), io::Error> {
        out_buffer.reserve(((len / BUFFER_SIZE) + 1) * BUFFER_SIZE);

        let compressor = self.compressor.unwrap_or_else(compressors::identity);
        compressor.compress(in_buffer, out_buffer, len)?;
        in_buffer.advance(len);

        debug!(
            "Decompressed {} bytes into {} bytes using {:?}",
            len,
            out_buffer.len(),
            compressor.name()
        );

        Ok(())
    }

    /// Set the `grpc-encoding` header with the compressor name
    pub(crate) fn set_headers(&self, headers: &mut http::HeaderMap) {
        match self.compressor {
            None => {},
            Some(compressor) => {
                headers.insert(ENCODING_HEADER, HeaderValue::from_static(compressor.name()));
            }
        }
    }
}
