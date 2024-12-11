use std::io;

use thiserror::Error;
use tonlib_core::types::TonHashParseError;
use tonlib_core::TonAddressParseError;

use crate::tl::{TlError, TonResult, TonResultDiscriminants};

#[derive(Error, Debug)]
pub enum TonClientError {
    #[error("Internal error ({0})")]
    InternalError(String),

    #[error("Tonlib error (Method: {method}, code: {code}, message: {message})")]
    TonlibError {
        method: &'static str,
        code: i32,
        message: String,
    },

    #[error("Invalid argument ({0})")]
    InvalidArgument(String),

    #[error("Unexpected TonResult (Actual: {actual}, expected: {expected})")]
    UnexpectedTonResult {
        actual: TonResultDiscriminants,
        expected: TonResultDiscriminants,
    },

    #[error("IO error ({0})")]
    Io(#[from] io::Error),

    #[error("TlError: ({0})")]
    TlError(#[from] TlError),

    #[error("TonAddressParseError: ({0})")]
    TonAddressParseError(#[from] TonAddressParseError),

    #[error("TonHash parse error ({0})")]
    TonHashParseError(#[from] TonHashParseError),
}

impl TonClientError {
    pub fn unexpected_ton_result(
        expected: TonResultDiscriminants,
        actual: TonResult,
    ) -> TonClientError {
        TonClientError::UnexpectedTonResult {
            actual: actual.into(),
            expected,
        }
    }
}
