use std::fmt::Debug;

use anyhow::{anyhow, Result};

/// Wraps a `Result` and provides helper methods for testing
pub struct TestResult(Result<()>);

impl Default for TestResult {
    fn default() -> Self {
        TestResult(Ok(()))
    }
}

impl From<TestResult> for Result<()> {
    fn from(result: TestResult) -> Self {
        result.0
    }
}

impl TestResult {
    /// Applies the AND operation to the given results
    ///
    /// If `result` contains already an error then `other_result` is
    /// ignored else if `other_result` contains an error then it is
    /// applied on `result`.
    pub fn combine<T, E>(&mut self, other_result: &Result<T, E>)
    where
        E: Debug,
    {
        if self.0.is_ok() {
            if let Err(error) = other_result {
                self.0 = Err(anyhow!("{:?}", error))
            }
        }
    }
}
