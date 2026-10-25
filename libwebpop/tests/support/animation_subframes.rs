use super::{Case, SEED, random};
use libwebpop::mux::anim_encode::*;

fn output(out: &mut Vec<u32>, rect: FrameRectangle, view: Option<ArgbView<'_>>) {
    out.extend([
        rect.x_offset as u32,
        rect.y_offset as u32,
        rect.width as u32,
        rect.height as u32,
        u32::from(view.is_some()),
    ]);
    if let Some(view) = view {
        out.extend([
            view.width() as u32,
            view.height() as u32,
            view.stride() as u32,
        ]);
        for y in 0..view.height() as usize {
            out.extend_from_slice(view.row(y).unwrap());
        }
    }
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    let len = ((a[2] - 1) * a[3] + a[1]) as usize;
    let src = ArgbView::new(&a[15..15 + len], a[1] as i32, a[2] as i32, a[3] as usize).unwrap();
    let dst = ArgbView::new(&a[15 + len..], a[1] as i32, a[2] as i32, a[4] as usize).unwrap();
    let mut rect = FrameRectangle {
        x_offset: a[11] as i32,
        y_offset: a[12] as i32,
        width: a[13] as i32,
        height: a[14] as i32,
    };
    let mut out = vec![1];
    match a[0] {
        0 => match dst.sub_view(rect) {
            Ok(view) => output(&mut out, rect, Some(view)),
            Err(_) => return vec![0],
        },
        1 => {
            let view = GetSubRect(
                src,
                dst,
                a[5] != 0,
                a[6] != 0,
                a[7] != 0,
                a[8] != 0,
                f32::from_bits(a[9]),
                &mut rect,
            )
            .unwrap();
            output(&mut out, rect, view);
        }
        2 => {
            let mut params = SubFrameParams::new(a[10] != 0, a[7] != 0);
            GetSubRects(
                src,
                dst,
                a[5] != 0,
                a[6] != 0,
                f32::from_bits(a[9]),
                &mut params,
            )
            .unwrap();
            out.extend([
                u32::from(params.should_try),
                u32::from(params.empty_rect_allowed),
            ]);
            output(&mut out, params.rect_ll, params.sub_frame_ll);
            output(&mut out, params.rect_lossy, params.sub_frame_lossy);
        }
        _ => unreachable!(),
    }
    out
}

fn base(width: u32, height: u32, pattern: u32) -> Case {
    let (s, d) = (width + pattern % 3, width + (pattern + 1) % 4);
    let mut input = vec![
        2,
        width,
        height,
        s,
        d,
        0,
        0,
        1,
        1,
        75f32.to_bits(),
        1,
        0,
        0,
        width,
        height,
    ];
    let mut src = vec![0xdeadc0de; ((height - 1) * s + width) as usize];
    let mut dst = vec![0xaabbccdd; ((height - 1) * d + width) as usize];
    let mut state = SEED;
    for y in 0..height {
        for x in 0..width {
            let a = if pattern == 8 { 0 } else { random(&mut state) };
            let mut b = a;
            match pattern {
                0 => {}
                1 => b ^= u32::from(x == width / 2 && y == height / 2) * 0x40,
                2 => b ^= u32::from(x + 1 == width && y + 1 == height),
                3 => b ^= u32::from(x == 0 || y == 0) * 0xff000000,
                4 => {
                    b ^= u32::from((x == 0 && y == 0) || (x + 1 == width && y + 1 == height))
                        * 0x01000000
                }
                5 => b ^= 0x010101,
                6 => b = (a ^ 0x404040) & 0x00ffffff,
                7 => b = random(&mut state) | 0xff000000,
                8 => {
                    if x == width / 2 && y == height / 2 {
                        b = 0x80000000;
                    }
                }
                _ => b = random(&mut state),
            }
            src[(y * s + x) as usize] = if pattern == 6 { a & 0x00ffffff } else { a };
            dst[(y * d + x) as usize] = b;
        }
    }
    input.extend(src);
    input.extend(dst);
    Case::new("AnimationSubframes", input)
}

pub fn differential_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (w, h) in [(1, 1), (1, 17), (17, 1), (3, 5), (8, 8), (17, 19)] {
        for pattern in 0..10 {
            let base = base(w, h, pattern);
            for key in [0, 1] {
                for first in [0, 1] {
                    for empty in [0, 1] {
                        for quality in [0f32, 24.5, 75., 98.5, 100.] {
                            let mut case = base.clone();
                            case.input[5..8].copy_from_slice(&[key, first, empty]);
                            case.input[9] = quality.to_bits();
                            case.input[10] = (pattern + key + first + empty) % 2;
                            cases.push(case.clone());
                            case.input[0] = 1;
                            for lossless in [0, 1] {
                                case.input[8] = lossless;
                                for rect in [
                                    [0, 0, w, h],
                                    [w / 2, h / 2, w - w / 2, h - h / 2],
                                    [0, 0, 0, 0],
                                ] {
                                    case.input[11..15].copy_from_slice(&rect);
                                    cases.push(case.clone());
                                }
                            }
                        }
                    }
                }
            }
            for rect in [
                [0, 0, w, h],
                [w / 2, h / 2, w - w / 2, h - h / 2],
                [w - 1, h - 1, 1, 1],
                [w, 0, 1, 1],
                [0, h, 1, 1],
                [0, 0, 0, h],
                [0, 0, w, 0],
                [u32::MAX, 0, 1, 1],
                [0, 0, i32::MAX as u32, i32::MAX as u32],
            ] {
                let mut case = base.clone();
                case.input[0] = 0;
                case.input[11..15].copy_from_slice(&rect);
                cases.push(case);
            }
        }
    }
    cases
}

pub fn fixture_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    // All key/first/empty/lossless routes, including unchanged, nearly equal,
    // alpha changes and odd-origin subrectangles, without huge fixture inputs.
    for pattern in [0, 1, 5, 6, 8] {
        for flags in 0..8 {
            let mut case = base(3, 5, pattern);
            case.input[5..8].copy_from_slice(&[flags & 1, flags >> 1 & 1, flags >> 2 & 1]);
            case.input[9] = if flags & 1 == 0 { 24.5f32 } else { 100f32 }.to_bits();
            case.input[10] = flags & 1;
            cases.push(case);
        }
    }
    for op in [0, 1] {
        for lossless in [0, 1] {
            let mut case = base(3, 5, 1);
            case.input[0] = op;
            case.input[8] = lossless;
            case.input[11..15].copy_from_slice(&[1, 1, 2, 4]);
            cases.push(case);
        }
    }
    for rect in [[3, 0, 1, 1], [0, 0, 0, 0], [u32::MAX, 0, 1, 1]] {
        let mut case = base(3, 5, 0);
        case.input[0] = 0;
        case.input[11..15].copy_from_slice(&rect);
        cases.push(case);
    }
    cases
}
