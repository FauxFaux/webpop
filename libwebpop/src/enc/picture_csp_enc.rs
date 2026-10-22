//! Checked positive-stride geometry for the planned RGBA importer.
//! Corresponds to upstream's Import / CheckStride / WebPValidatePicture checks.

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RgbaInputError {
    #[error("width and height must be in 1..=16383")]
    InvalidDimensions,
    #[error("stride is smaller than width * 4")]
    InvalidStride,
    #[error("RGBA input length arithmetic overflowed")]
    SizeOverflow,
    #[error("RGBA buffer is shorter than the last pixel")]
    InputTooShort,
}

/// Validate without allocating or reading input. Returns the minimum byte
/// length, which deliberately excludes padding after the last row.
pub fn checked_rgba_geometry(
    input_len: usize,
    width: u32,
    height: u32,
    stride: usize,
) -> Result<usize, RgbaInputError> {
    if !(1..=16383).contains(&width) || !(1..=16383).contains(&height) {
        return Err(RgbaInputError::InvalidDimensions);
    }
    let row_bytes = (width as usize)
        .checked_mul(4)
        .ok_or(RgbaInputError::SizeOverflow)?;
    if stride < row_bytes {
        return Err(RgbaInputError::InvalidStride);
    }
    let required = (height as usize - 1)
        .checked_mul(stride)
        .and_then(|offset| offset.checked_add(row_bytes))
        .ok_or(RgbaInputError::SizeOverflow)?;
    if input_len < required {
        return Err(RgbaInputError::InputTooShort);
    }
    Ok(required)
}
