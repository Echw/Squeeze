use crate::types::OptimizeError;

#[derive(Default)]
pub(crate) struct JpegMetadata {
    pub icc: Option<Vec<u8>>,
    pub exif: Option<Vec<u8>>,
}

pub(crate) fn extract(input: &[u8]) -> Result<JpegMetadata, OptimizeError> {
    if !input.starts_with(&[0xff, 0xd8]) {
        return Ok(JpegMetadata::default());
    }
    let mut cursor = 2;
    let mut exif = None;
    let mut icc_parts: Vec<(u8, u8, Vec<u8>)> = Vec::new();
    while cursor + 4 <= input.len() {
        if input[cursor] != 0xff {
            break;
        }
        let marker = input[cursor + 1];
        cursor += 2;
        if marker == 0xda || marker == 0xd9 {
            break;
        }
        if matches!(marker, 0x01 | 0xd0..=0xd7) {
            continue;
        }
        let length = u16::from_be_bytes([input[cursor], input[cursor + 1]]) as usize;
        if length < 2 || cursor + length > input.len() {
            break;
        }
        let payload = &input[cursor + 2..cursor + length];
        if marker == 0xe1 && payload.starts_with(b"Exif\0\0") {
            exif = Some(payload[6..].to_vec());
        }
        if marker == 0xe2 && payload.starts_with(b"ICC_PROFILE\0") && payload.len() > 14 {
            icc_parts.push((payload[12], payload[13], payload[14..].to_vec()));
        }
        cursor += length;
    }
    icc_parts.sort_by_key(|(order, _, _)| *order);
    let icc = if icc_parts.is_empty() {
        None
    } else {
        let expected = icc_parts[0].1;
        let complete = expected > 0
            && icc_parts.len() == expected as usize
            && icc_parts
                .iter()
                .enumerate()
                .all(|(index, (order, total, _))| {
                    *order as usize == index + 1 && *total == expected
                });
        if !complete {
            return Err(OptimizeError::InvalidColorProfile);
        }
        let bytes = icc_parts
            .into_iter()
            .flat_map(|(_, _, bytes)| bytes)
            .collect::<Vec<_>>();
        validate_icc(&bytes)?;
        Some(bytes)
    };
    Ok(JpegMetadata { icc, exif })
}

fn validate_icc(profile: &[u8]) -> Result<(), OptimizeError> {
    if profile.len() < 128 || profile.get(36..40) != Some(b"acsp") {
        return Err(OptimizeError::InvalidColorProfile);
    }
    let declared = u32::from_be_bytes([profile[0], profile[1], profile[2], profile[3]]) as usize;
    if declared < 128 || declared > profile.len() {
        return Err(OptimizeError::InvalidColorProfile);
    }
    Ok(())
}
