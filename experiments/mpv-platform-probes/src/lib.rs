//! Native texture-interop **test-readback**, not playback qualification.

#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

pub const WIDTH: usize = 4;
pub const HEIGHT: usize = 4;
pub const ROW_BYTES: usize = 256;
/// Deliberately unequal channels and quadrants, in GL bottom-row-first order.
pub const COLORS: [[u8; 4]; 4] = [
    [17, 53, 101, 255],
    [149, 29, 71, 255],
    [43, 181, 113, 255],
    [211, 137, 7, 255],
];

/// Require a unique, nonzero registry identity, never a renderer-family guess.
pub fn matching_registry(renderer: u32, candidates: &[(u32, u64)]) -> Result<u64, String> {
    let matches: Vec<_> = candidates
        .iter()
        .filter(|(id, _)| *id == renderer)
        .map(|(_, registry)| *registry)
        .collect();
    match matches.as_slice() {
        [id] if *id != 0 => Ok(*id),
        _ => Err("CGL renderer registry identity is missing or ambiguous".into()),
    }
}

/// Validate actual BGRA memory including row layout, without hiding a Y flip.
/// Metal row zero must correspond to GL's y=0, not UI top-left coordinates.
pub fn validate_bgra(bytes: &[u8], row_bytes: usize) -> Result<(), String> {
    validate_bgra_rows(bytes, row_bytes, true)
}

/// Explicit native row policy, fixed by the caller before inspecting bytes.
/// These CGL and ordinary ANGLE pbuffer probes both require GL-y-zero-first;
/// native row coordinates are not a UI top-left orientation guarantee.
/// Never detect/accept either orientation automatically.
pub fn validate_bgra_rows(
    bytes: &[u8],
    row_bytes: usize,
    gl_y_zero_first: bool,
) -> Result<(), String> {
    if row_bytes < WIDTH * 4
        || row_bytes
            .checked_mul(HEIGHT)
            .is_none_or(|n| bytes.len() < n)
    {
        return Err("invalid test-readback layout".into());
    }
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let gl_y = if gl_y_zero_first { y } else { HEIGHT - 1 - y };
            let rgba = COLORS[(gl_y / 2) * 2 + x / 2];
            let expected = [rgba[2], rgba[1], rgba[0], rgba[3]];
            let offset = y * row_bytes + x * 4;
            if bytes[offset..offset + 4] != expected {
                return Err(format!("BGRA/orientation mismatch at x={x} y={y}"));
            }
        }
    }
    Ok(())
}

/// Native resources are created, used, and retired entirely on this worker.
pub fn run_test_readback() -> Result<(), String> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        #[cfg(target_os = "macos")]
        let run = macos::run;
        #[cfg(target_os = "windows")]
        let run = windows::run;
        std::thread::Builder::new()
            .name("native-texture-probe".into())
            .spawn(run)
            .map_err(|_| "cannot start probe worker".to_string())?
            .join()
            .map_err(|_| "probe worker panicked".to_string())?
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err("unsupported platform: native test-readback requires macOS or Windows".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image() -> Vec<u8> {
        let mut bytes = vec![0xa5; ROW_BYTES * HEIGHT];
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let c = COLORS[(y / 2) * 2 + x / 2];
                let i = y * ROW_BYTES + x * 4;
                bytes[i..i + 4].copy_from_slice(&[c[2], c[1], c[0], c[3]]);
            }
        }
        bytes
    }

    #[test]
    fn exact_bgra_and_padded_layout() {
        assert!(validate_bgra(&image(), ROW_BYTES).is_ok());
        assert!(validate_bgra(&[], ROW_BYTES).is_err());
        assert!(validate_bgra(&image(), 1).is_err());
        assert!(validate_bgra(&image(), usize::MAX).is_err());
    }

    #[test]
    fn rejects_channel_swap_and_vertical_flip() {
        let original = image();
        let mut swapped = original.clone();
        swapped.swap(0, 2);
        assert!(validate_bgra(&swapped, ROW_BYTES).is_err());
        let mut flipped = original.clone();
        for y in 0..HEIGHT {
            flipped[y * ROW_BYTES..(y + 1) * ROW_BYTES]
                .copy_from_slice(&original[(HEIGHT - 1 - y) * ROW_BYTES..(HEIGHT - y) * ROW_BYTES]);
        }
        assert!(validate_bgra(&flipped, ROW_BYTES).is_err());
        assert!(validate_bgra_rows(&flipped, ROW_BYTES, false).is_ok());
        assert!(validate_bgra_rows(&original, ROW_BYTES, false).is_err());
    }

    #[test]
    fn rejects_unproven_identity() {
        assert_eq!(matching_registry(7, &[(7, 42)]), Ok(42));
        assert!(matching_registry(7, &[(8, 42)]).is_err());
        assert!(matching_registry(7, &[(7, 0)]).is_err());
        assert!(matching_registry(7, &[(7, 42), (7, 42)]).is_err());
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    #[test]
    fn unsupported_is_failure() {
        assert!(run_test_readback().is_err());
    }
}
