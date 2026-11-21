use std::fmt;
use std::fmt::{Display, Formatter};
use thousands::Separable;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Dollars<T = i64>(pub T);

impl<T: Copy + Into<i128>> Display for Dollars<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let abs_val = self.0.into().unsigned_abs();
        let dollars = abs_val / 100;
        let cents = abs_val % 100;

        write!(f, "${}.{cents:02}", dollars.separate_with_commas())
    }
}
