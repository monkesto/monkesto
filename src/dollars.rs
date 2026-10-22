use std::fmt;
use std::fmt::{Display, Formatter};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Dollars(pub i64);

impl Display for Dollars {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let abs_val = self.0.unsigned_abs();
        let dollars = abs_val / 100;
        let cents = abs_val % 100;

        write!(f, "${dollars}.{cents:02}")
    }
}
