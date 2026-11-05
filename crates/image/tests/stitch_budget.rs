//! IMAGE-LIMIT-38-01: growing the verified Scroll surface must not allocate a
//! fresh, ever-larger buffer for every frame.
//!
//! A 24,000,000-pixel stitched document is 96 MB packed. Rebuilding that
//! surface once per accepted frame copies the whole document per frame and
//! leaves the allocator holding every intermediate size, which pushed the
//! observed physical footprint of a 24 M export past its 2 GiB budget even
//! though the live data never exceeded a few surfaces. The counting allocator
//! below records only *record-breaking* large allocations: a request larger
//! than every earlier one cannot be served from the free pool, so those are
//! exactly the mappings that accumulate as resident pages.
use arcshot_core::{CapturedFrame, ScreenGeometry};
use arcshot_image::{StitchBuilder, StitchDecision};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};

const MIN_PROBE_BYTES: usize = 1 << 20;

static TRACKING: AtomicBool = AtomicBool::new(false);
static LARGEST: AtomicUsize = AtomicUsize::new(0);
static RECORD_BYTES: AtomicUsize = AtomicUsize::new(0);
static RECORD_COUNT: AtomicUsize = AtomicUsize::new(0);

struct CountingAllocator;

impl CountingAllocator {
    fn record(size: usize) {
        if !TRACKING.load(Relaxed) || size < MIN_PROBE_BYTES {
            return;
        }
        if size > LARGEST.fetch_max(size, Relaxed) {
            RECORD_BYTES.fetch_add(size, Relaxed);
            RECORD_COUNT.fetch_add(1, Relaxed);
        }
    }
}

// SAFETY: every call is forwarded unchanged to the system allocator; the probe
// only observes request sizes.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        Self::record(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        Self::record(new_size);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

const WIDTH: u32 = 1500;
const VIEWPORT_ROWS: u32 = 384;
const STEP_ROWS: u32 = 128;
const DOCUMENT_ROWS: u32 = 16_000;

fn row_bytes(seed: u32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(WIDTH as usize * 4);
    let mut state = u64::from(seed).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    for _ in 0..WIDTH as usize {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let value = state as u8;
        bytes.extend_from_slice(&[value, value.wrapping_mul(3), (state >> 8) as u8, 255]);
    }
    bytes
}

fn viewport(top_row: u32) -> CapturedFrame {
    let mut bgra = Vec::with_capacity(WIDTH as usize * 4 * VIEWPORT_ROWS as usize);
    for row in 0..VIEWPORT_ROWS {
        bgra.extend_from_slice(&row_bytes(top_row + row));
    }
    let geometry = ScreenGeometry::try_new(
        1,
        0.0,
        0.0,
        f64::from(WIDTH) / 2.0,
        f64::from(VIEWPORT_ROWS) / 2.0,
        2.0,
    )
    .expect("viewport geometry");
    CapturedFrame::try_from_srgb_bgra(geometry, WIDTH, VIEWPORT_ROWS, WIDTH as usize * 4, bgra)
        .expect("viewport frame")
}

#[test]
fn image_limit_38_01_stitching_24m_reuses_its_surface_instead_of_growing_it_per_frame() {
    let origin = viewport(0);
    let mut builder = StitchBuilder::new(&origin).expect("origin");
    // The session keeps the latest verified document, so every accepted frame
    // materializes one. That copy is part of the measured pattern.
    let mut verified = origin;
    TRACKING.store(true, Relaxed);
    let mut top = 0;
    while builder.height() < DOCUMENT_ROWS {
        top += STEP_ROWS;
        let decision = builder.submit(&viewport(top)).expect("extension");
        assert!(matches!(decision, StitchDecision::Extended { .. }));
        verified = builder.current_frame().expect("verified document");
        let preview = builder.preview(4_000_000).expect("preview");
        std::hint::black_box(&preview.bgra);
    }
    TRACKING.store(false, Relaxed);

    assert_eq!(builder.height(), DOCUMENT_ROWS);
    assert_eq!(
        u64::from(WIDTH) * u64::from(DOCUMENT_ROWS),
        24_000_000,
        "the fixture is the V1 maximum document"
    );
    let document_bytes = verified.bgra().len();
    let records = RECORD_COUNT.load(Relaxed);
    let record_bytes = RECORD_BYTES.load(Relaxed);

    println!("probe records={records} bytes={record_bytes} document={document_bytes}");
    assert!(
        records <= 12,
        "building one 24 M document made {records} record-breaking large allocations"
    );
    assert!(
        record_bytes <= document_bytes * 3,
        "record-breaking allocations summed to {record_bytes} bytes for a \
         {document_bytes}-byte document"
    );
    // Content order survives the in-place shifts, not just the geometry.
    let frame = verified.bgra();
    let stride = verified.stride();
    assert_eq!(&frame[..stride], &row_bytes(0)[..]);
    let last = (DOCUMENT_ROWS as usize - 1) * stride;
    assert_eq!(
        &frame[last..last + stride],
        &row_bytes(DOCUMENT_ROWS - 1)[..]
    );
}
