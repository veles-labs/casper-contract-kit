use casper_types::{ApiError, CLValueError};

const USER_BASE: u16 = 56900;

#[repr(u16)]
#[derive(Debug, PartialEq, Eq)]
pub enum UniversalError {
    Panic = USER_BASE,
    InvalidContext,
    URefAlreadyInitialized,
    Other(ApiError) = 0,
}

impl From<ApiError> for UniversalError {
    fn from(api_error: ApiError) -> Self {
        Self::Other(api_error)
    }
}

impl UniversalError {
    fn discriminant(&self) -> u16 {
        // SAFETY: Because `Self` is marked `repr(u8)`, its layout is a `repr(C)` `union`
        // between `repr(C)` structs, each of which has the `u8` discriminant as its first
        // field, so we can read the discriminant without offsetting the pointer.
        unsafe { *<*const Self>::from(self).cast::<u16>() }
    }
}

impl From<CLValueError> for UniversalError {
    fn from(error: CLValueError) -> Self {
        let api_error: ApiError = error.into();
        UniversalError::Other(api_error)
    }
}

impl From<UniversalError> for crate::casper_types::ApiError {
    fn from(error: UniversalError) -> Self {
        let result = match error {
            UniversalError::Other(api_error) => api_error,
            other => ApiError::User(other.discriminant()),
        };
        debug_assert_ne!(result, ApiError::User(u16::MAX));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::UniversalError;
    use casper_types::ApiError;

    #[test]
    fn test_discriminants() {
        assert_eq!(UniversalError::Panic.discriminant(), 56900);
        assert_eq!(UniversalError::InvalidContext.discriminant(), 56901);
        assert_eq!(UniversalError::URefAlreadyInitialized.discriminant(), 56902);
        assert_eq!(
            UniversalError::Other(ApiError::User(12345)).discriminant(),
            0
        );
    }

    #[test]
    fn test_conversion() {
        let error = UniversalError::InvalidContext;
        let api_error: ApiError = error.into();
        assert_eq!(api_error, ApiError::User(56901));
    }
}
