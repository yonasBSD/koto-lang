use crate::{prelude::*, Ptr, Result};
use std::ops::{Deref, Range};

/// The Tuple type used by the Koto runtime
#[derive(Clone)]
pub struct KTuple(Inner);

// Either the full tuple, a slice with 16bit bounds, or a slice with larger bounds
#[derive(Clone)]
enum Inner {
    Full(Ptr<Vec<KValue>>),
    Slice(Ptr<TupleSlice>),
    Slice16(TupleSlice16),
}

impl KTuple {
    /// Returns a new tuple with shared data and with restricted bounds
    ///
    /// The provided bounds should have indices relative to the current tuple's bounds
    /// (i.e. instead of relative to the underlying shared tuple data), so it follows that the
    /// result will always be a subset of the input tuple.
    pub fn make_sub_tuple(&self, mut new_bounds: Range<usize>) -> Option<Self> {
        let slice = match &self.0 {
            Inner::Full(data) => TupleSlice::from(data.clone()),
            Inner::Slice(slice) => slice.deref().clone(),
            Inner::Slice16(slice) => TupleSlice::from(slice.clone()),
        };

        new_bounds.start += slice.bounds.start;
        new_bounds.end += slice.bounds.start;

        if new_bounds.end <= slice.bounds.end && slice.get(new_bounds.clone()).is_some() {
            let result = TupleSlice {
                data: slice.data.clone(),
                bounds: new_bounds,
            };
            Some(result.into())
        } else {
            None
        }
    }

    /// Returns true if the tuple contains only immutable values
    pub fn is_hashable(&self) -> bool {
        self.iter().all(KValue::is_hashable)
    }

    /// Removes and returns the first value in the tuple
    ///
    /// The internal bounds of the tuple are adjusted to 'remove' the first element;
    /// no change is made to the underlying tuple data.
    pub fn pop_front(&mut self) -> Option<KValue> {
        match &mut self.0 {
            Inner::Full(data) => {
                if let Some(value) = data.first().cloned() {
                    *self = Self::from(TupleSlice {
                        data: data.clone(),
                        bounds: 1..data.len(),
                    });
                    Some(value)
                } else {
                    None
                }
            }
            Inner::Slice(slice) => {
                if let Some(value) = slice.first().cloned() {
                    Ptr::make_mut(slice).bounds.start += 1;
                    Some(value)
                } else {
                    None
                }
            }
            Inner::Slice16(slice) => {
                if let Some(value) = slice.first().cloned() {
                    slice.bounds.start += 1;
                    Some(value)
                } else {
                    None
                }
            }
        }
    }

    /// Removes and returns the last value in the tuple
    ///
    /// The internal bounds of the tuple are adjusted to 'remove' the first element;
    /// no change is made to the underlying tuple data.
    pub fn pop_back(&mut self) -> Option<KValue> {
        match &mut self.0 {
            Inner::Full(data) => {
                if let Some(value) = data.last().cloned() {
                    *self = Self::from(TupleSlice {
                        data: data.clone(),
                        bounds: 0..data.len() - 1,
                    });
                    Some(value)
                } else {
                    None
                }
            }
            Inner::Slice(slice) => {
                if let Some(value) = slice.last().cloned() {
                    Ptr::make_mut(slice).bounds.end -= 1;
                    Some(value)
                } else {
                    None
                }
            }
            Inner::Slice16(slice) => {
                if let Some(value) = slice.last().cloned() {
                    slice.bounds.end -= 1;
                    Some(value)
                } else {
                    None
                }
            }
        }
    }

    /// Renders the tuple into the provided display context
    pub fn display(&self, ctx: &mut DisplayContext) -> Result<()> {
        let id = Ptr::address(match &self.0 {
            Inner::Full(data) => data,
            Inner::Slice(slice) => &slice.data,
            Inner::Slice16(slice) => &slice.data,
        });
        ctx.push_container(id);
        ctx.append('(');

        for (i, value) in self.iter().enumerate() {
            if i > 0 {
                ctx.append(", ");
            }
            value.display(ctx)?;
        }

        ctx.append(')');
        ctx.pop_container();

        Ok(())
    }
}

impl Deref for KTuple {
    type Target = [KValue];

    fn deref(&self) -> &[KValue] {
        match &self.0 {
            Inner::Full(data) => data,
            Inner::Slice16(slice) => slice.deref(),
            Inner::Slice(slice) => slice.deref(),
        }
    }
}

impl Default for KTuple {
    fn default() -> Self {
        Vec::new().into()
    }
}

impl From<Vec<KValue>> for KTuple {
    fn from(data: Vec<KValue>) -> Self {
        Self(Inner::Full(data.into()))
    }
}

impl From<&[KValue]> for KTuple {
    fn from(data: &[KValue]) -> Self {
        Self(Inner::Full(data.to_vec().into()))
    }
}

impl<const N: usize> From<&[KValue; N]> for KTuple {
    fn from(data: &[KValue; N]) -> Self {
        Self::from(data.as_slice())
    }
}

impl From<TupleSlice> for KTuple {
    fn from(slice: TupleSlice) -> Self {
        match TupleSlice16::try_from(slice) {
            Ok(slice16) => Self::from(slice16),
            Err(slice) => Self(Inner::Slice(slice.into())),
        }
    }
}

impl From<TupleSlice16> for KTuple {
    fn from(slice: TupleSlice16) -> Self {
        Self(Inner::Slice16(slice.into()))
    }
}

#[derive(Clone)]
struct TupleSlice {
    data: Ptr<Vec<KValue>>,
    bounds: Range<usize>,
}

impl Deref for TupleSlice {
    type Target = [KValue];

    fn deref(&self) -> &[KValue] {
        // Safety: bounds have already been checked in the From impls and make_sub_tuple
        unsafe { self.data.get_unchecked(self.bounds.clone()) }
    }
}

impl From<Ptr<Vec<KValue>>> for TupleSlice {
    fn from(data: Ptr<Vec<KValue>>) -> Self {
        let bounds = 0..data.len();
        Self { data, bounds }
    }
}

impl From<TupleSlice16> for TupleSlice {
    fn from(slice: TupleSlice16) -> Self {
        Self {
            data: slice.data,
            bounds: u16_to_usize_range(slice.bounds),
        }
    }
}

// A slice with 16bit bounds, allowing it to be stored in KTuple without the overhead of additional
// allocation.
//
// Q. Why 16bit bounds, when the non-allocated slice in KString has 32bit bounds?
// A. Because KValue can only have a single variant that has a size of 24 bytes to allow niche
//    optimization to be performed, reusing KString's niche.
//    If KTuple also had a size of 24 bytes, then niche optimization would fail (the compiler
//    currently doesn't allow niches from variants with the same layout to be shared).
// Q. Ok, but why not have 32bit bounds for tuple slices instead of string slices?
// A. A call has to be made here, and I think maybe making string slices 16bit instead would be more
//    sensible, opening the possibility of an alternative variant increasing in size to 24 and
//    taking on the niche-provider role.
#[derive(Clone)]
struct TupleSlice16 {
    data: Ptr<Vec<KValue>>,
    bounds: Range<u16>,
    // A placeholder for the compiler to be able to perform niche optimization on KTuple
    // so that its size can be kept down to 16 bytes.
    // Although the size of TupleSlice16's fields is 12 on 64bit platforms,
    // with padding the overall size is 16. Niche optimization isn't allowed to use
    // padding bytes, so this 1 byte placeholder gives the compiler a legitimate spot
    // to place the niche.
    // Without niche optimization, the size of KTuple increases to 24 bytes,
    // which then causes KValue's size to increase to 32, which is over the limit.
    _niche_placeholder: ZeroU8,
}

impl Deref for TupleSlice16 {
    type Target = [KValue];

    fn deref(&self) -> &[KValue] {
        // Safety: bounds have already been checked in the TryFrom impl
        unsafe {
            self.data
                .get_unchecked(u16_to_usize_range(self.bounds.clone()))
        }
    }
}

impl TryFrom<TupleSlice> for TupleSlice16 {
    type Error = TupleSlice;

    fn try_from(slice: TupleSlice) -> std::result::Result<Self, Self::Error> {
        usize_to_u16_range(slice.bounds.clone())
            .map(|bounds| Self {
                data: slice.data.clone(),
                bounds,
                _niche_placeholder: ZeroU8::Zero,
            })
            .ok_or(slice)
    }
}

#[repr(u8)]
#[derive(Clone)]
enum ZeroU8 {
    Zero = 0,
}

fn u16_to_usize_range(r: Range<u16>) -> Range<usize> {
    r.start as usize..r.end as usize
}

fn usize_to_u16_range(r: Range<usize>) -> Option<Range<u16>> {
    match (u16::try_from(r.start), u16::try_from(r.end)) {
        (Ok(start), Ok(end)) => Some(start..end),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tuple_mem_size() {
        assert!(std::mem::size_of::<KTuple>() <= 16);
    }
}
