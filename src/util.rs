use crate::monkesto_error::MonkestoError;
use url::Url;

pub trait GetLocation {
    /// A test function to get the Location header of a response as a string.
    ///
    /// # Panics
    /// This will panic if the Location header is not found or if the value is not valid UTF-8.
    fn get_location(&self) -> String;
}

impl GetLocation for axum_test::TestResponse {
    fn get_location(&self) -> String {
        self.headers()
            .get("Location")
            .expect("Location header not found")
            .to_str()
            .expect("Location header value is not valid UTF-8")
            .to_string()
    }
}

pub trait GetError: GetLocation {
    /// A test function to get an error from a response.
    ///
    /// # Returns
    ///
    /// Returns Some if the url contains an error query parameter
    /// and None otherwise
    ///
    /// # Panics
    /// This will panic if the Location header is not found, if the value is not valid UTF-8, or if the Location header is not a valid URL.
    fn get_error(&self) -> Option<MonkestoError>;

    /// Asserts that the response has the given error.
    fn assert_error(&self, expected_error: MonkestoError) {
        assert!(
            self.get_error()
                .is_some_and(|error| error == expected_error)
        );
    }

    /// Asserts that the response does not have an error.
    fn assert_ok(&self) {
        assert!(self.get_error().is_none());
    }
}

impl GetError for axum_test::TestResponse {
    fn get_error(&self) -> Option<MonkestoError> {
        // the location returned is relative, but the parse function expects an absolute URL
        let url = Url::parse("monkesto.com")
            .expect("Invalid base URL")
            .join(&self.get_location())
            .expect("Invalid URL");

        url.query_pairs()
            .find(|(key, _)| key == "error")
            .map(|(_, value)| MonkestoError::decode(&value))
    }
}
