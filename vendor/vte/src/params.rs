//! Fixed size parameters list with optional subparameters.

use core::fmt::{self, Debug, Formatter};

pub(crate) const MAX_PARAMS: usize = 32;

#[derive(Default)]
pub struct Params {
    /// Number of subparameters for each parameter.
    ///
    /// For each entry in the `params` slice, this stores the length of the
    /// param as number of subparams at the same index as the param in the
    /// `params` slice.
    ///
    /// At the subparam positions the length will always be `0`.
    subparams: [u8; MAX_PARAMS],

    /// All parameters and subparameters.
    params: [u16; MAX_PARAMS],

    /// Number of suparameters in the current parameter.
    current_subparams: u8,

    /// Total number of parameters and subparameters.
    len: usize,

    /// b-terminal(#777): 마지막 매개변수 자리에 숫자가 하나도 없었는가(`CSI $ p`, `CSI 4 ; $ p`). 맨 끝에 밀어 넣은
    /// 빈 값을 구분하려는 것이다. Ghostty 파서는 숫자 없는 끝 매개변수를 세지 않는다(Parser.zig 378줄 `param_acc_idx > 0`).
    trailing_empty: bool,
}

impl Params {
    /// Returns the number of parameters.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// b-terminal(#777): 마지막 매개변수 자리에 숫자가 없었으면 `true`(`len()`에는 그래도 센다).
    #[inline]
    pub fn trailing_empty(&self) -> bool {
        self.trailing_empty
    }

    /// Returns `true` if there are no parameters present.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns an iterator over all parameters and subparameters.
    #[inline]
    pub fn iter(&self) -> ParamsIter<'_> {
        ParamsIter::new(self)
    }

    /// Returns `true` if there is no more space for additional parameters.
    #[inline]
    pub(crate) fn is_full(&self) -> bool {
        self.len == MAX_PARAMS
    }

    /// Clear all parameters.
    #[inline]
    pub(crate) fn clear(&mut self) {
        self.current_subparams = 0;
        self.len = 0;
        self.trailing_empty = false;
    }

    /// b-terminal(#777): 파서가 디스패치 직전에 정한다.
    #[inline]
    pub(crate) fn set_trailing_empty(&mut self, empty: bool) {
        self.trailing_empty = empty;
    }

    /// Add an additional parameter.
    #[inline]
    pub(crate) fn push(&mut self, item: u16) {
        self.subparams[self.len - self.current_subparams as usize] = self.current_subparams + 1;
        self.params[self.len] = item;
        self.current_subparams = 0;
        self.len += 1;
    }

    /// Add an additional subparameter to the current parameter.
    #[inline]
    pub(crate) fn extend(&mut self, item: u16) {
        self.subparams[self.len - self.current_subparams as usize] = self.current_subparams + 1;
        self.params[self.len] = item;
        self.current_subparams += 1;
        self.len += 1;
    }
}

impl<'a> IntoIterator for &'a Params {
    type IntoIter = ParamsIter<'a>;
    type Item = &'a [u16];

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Immutable subparameter iterator.
pub struct ParamsIter<'a> {
    params: &'a Params,
    index: usize,
}

impl<'a> ParamsIter<'a> {
    fn new(params: &'a Params) -> Self {
        Self { params, index: 0 }
    }
}

impl<'a> Iterator for ParamsIter<'a> {
    type Item = &'a [u16];

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.params.len() {
            return None;
        }

        // Get all subparameters for the current parameter.
        let num_subparams = self.params.subparams[self.index];
        let param = &self.params.params[self.index..self.index + num_subparams as usize];

        // Jump to the next parameter.
        self.index += num_subparams as usize;

        Some(param)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.params.len() - self.index;
        (remaining, Some(remaining))
    }
}

impl Debug for Params {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "[")?;

        for (i, param) in self.iter().enumerate() {
            if i != 0 {
                write!(f, ";")?;
            }

            for (i, subparam) in param.iter().enumerate() {
                if i != 0 {
                    write!(f, ":")?;
                }

                subparam.fmt(f)?;
            }
        }

        write!(f, "]")
    }
}
