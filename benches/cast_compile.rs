use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use crush_cast::Program;
use crush_frontend::{
    compiler::Compiler,
    optimizer::Optimizer,
    parser::{Lexer, Parser},
    semantics::SemanticAnalyzer,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const SAMPLE_RUNS: usize = 200;

struct TrackingAllocator;

static TRACKING: AtomicBool = AtomicBool::new(false);
static CURRENT_HEAP: AtomicUsize = AtomicUsize::new(0);
static PEAK_HEAP: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static REENTRANT: Cell<bool> = const { Cell::new(false) };
}

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            track_alloc(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        track_dealloc(layout.size());
    }

    unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = unsafe { System.realloc(ptr, old_layout, new_size) };
        if !new_ptr.is_null() {
            if new_size >= old_layout.size() {
                track_alloc(new_size - old_layout.size());
            } else {
                track_dealloc(old_layout.size() - new_size);
            }
        }
        new_ptr
    }
}

#[global_allocator]
static GLOBAL: TrackingAllocator = TrackingAllocator;

#[derive(Clone)]
struct Fixture {
    name: String,
    text: String,
    cast_json: String,
    cast_program: Program,
}

#[derive(Default)]
struct PhaseBreakdown {
    lex: Vec<Duration>,
    parse: Vec<Duration>,
    semantic: Vec<Duration>,
    optimize: Vec<Duration>,
    compile: Vec<Duration>,
}

#[derive(Clone)]
struct SampleSummary {
    p50: Duration,
    p95: Duration,
    peak_heap_bytes: usize,
}

fn track_alloc(size: usize) {
    if size == 0 || !TRACKING.load(Ordering::Relaxed) {
        return;
    }
    REENTRANT.with(|guard| {
        if guard.replace(true) {
            guard.set(true);
            return;
        }
        let current = CURRENT_HEAP.fetch_add(size, Ordering::Relaxed) + size;
        let mut observed = PEAK_HEAP.load(Ordering::Relaxed);
        while current > observed {
            match PEAK_HEAP.compare_exchange_weak(
                observed,
                current,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(next) => observed = next,
            }
        }
        guard.set(false);
    });
}

fn track_dealloc(size: usize) {
    if size == 0 || !TRACKING.load(Ordering::Relaxed) {
        return;
    }
    REENTRANT.with(|guard| {
        if guard.replace(true) {
            guard.set(true);
            return;
        }
        CURRENT_HEAP.fetch_sub(size, Ordering::Relaxed);
        guard.set(false);
    });
}

fn measure_peak<T>(f: impl FnOnce() -> T) -> (T, usize) {
    CURRENT_HEAP.store(0, Ordering::Relaxed);
    PEAK_HEAP.store(0, Ordering::Relaxed);
    TRACKING.store(true, Ordering::Relaxed);
    let result = f();
    TRACKING.store(false, Ordering::Relaxed);
    (result, PEAK_HEAP.load(Ordering::Relaxed))
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../benches/fixtures")
}

fn compile_text(source: &str) -> casm::Program {
    let program =
        Parser::parse(source).unwrap_or_else(|err| panic!("text fixture should parse: {:?}", err));
    let mut analyzer = SemanticAnalyzer::new();
    analyzer
        .check(&program)
        .expect("text fixture should type-check");
    let mut program = program;
    Optimizer::optimize(&mut program);
    let mut compiler = Compiler::new();
    compiler
        .compile(program)
        .expect("text fixture should compile")
}

fn compile_cast_json(cast_json: &str) -> casm::Program {
    let program: Program = serde_json::from_str(cast_json).expect("CAST fixture should decode");
    let mut program = program;
    let mut analyzer = SemanticAnalyzer::new();
    analyzer
        .check(&program)
        .expect("CAST fixture should type-check");
    Optimizer::optimize(&mut program);
    let mut compiler = Compiler::new();
    compiler
        .compile(program)
        .expect("CAST fixture should compile")
}

fn load_fixtures() -> Vec<Fixture> {
    let root = fixture_root();
    let mut fixtures = Vec::new();

    for index in 1..=20 {
        let name = format!("{index:02}");
        let text_path = root.join("text").join(format!("{name}.crush"));
        let cast_path = root.join("cast").join(format!("{name}.cast.json"));
        let text = fs::read_to_string(&text_path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", text_path.display()));
        let cast_json = fs::read_to_string(&cast_path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", cast_path.display()));
        let cast_program: Program = serde_json::from_str(&cast_json)
            .unwrap_or_else(|err| panic!("invalid CAST {}: {err}", cast_path.display()));
        let parsed = Parser::parse(&text)
            .unwrap_or_else(|err| panic!("invalid Crush {}: {:?}", text_path.display(), err));
        let parsed_json = serde_json::to_value(&parsed).expect("serialize parsed fixture");
        let cast_value = serde_json::to_value(&cast_program).expect("serialize CAST fixture");
        assert_eq!(
            parsed_json, cast_value,
            "{} must be the verbatim parsed AST serialization",
            name
        );
        fixtures.push(Fixture {
            name,
            text,
            cast_json,
            cast_program,
        });
    }

    fixtures
}

fn phase_breakdown(source: &str) -> PhaseBreakdown {
    let mut breakdown = PhaseBreakdown::default();

    for _ in 0..SAMPLE_RUNS {
        let start = Instant::now();
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("fixture should lex");
        breakdown.lex.push(start.elapsed());

        let start = Instant::now();
        let mut parser = Parser::new(tokens);
        let mut program = parser
            .parse_program_for_benchmark()
            .expect("fixture should parse");
        breakdown.parse.push(start.elapsed());

        let start = Instant::now();
        let mut analyzer = SemanticAnalyzer::new();
        analyzer.check(&program).expect("fixture should type-check");
        breakdown.semantic.push(start.elapsed());

        let start = Instant::now();
        Optimizer::optimize(&mut program);
        breakdown.optimize.push(start.elapsed());

        let start = Instant::now();
        let mut compiler = Compiler::new();
        let _casm = compiler.compile(program).expect("fixture should compile");
        breakdown.compile.push(start.elapsed());
    }

    breakdown
}

fn summarize(mut durations: Vec<Duration>, peak_heap_bytes: usize) -> SampleSummary {
    durations.sort_unstable();
    let p50_idx = durations.len() / 2;
    let p95_idx = ((durations.len() as f64 * 0.95).ceil() as usize)
        .saturating_sub(1)
        .min(durations.len() - 1);
    SampleSummary {
        p50: durations[p50_idx],
        p95: durations[p95_idx],
        peak_heap_bytes,
    }
}

fn sample_path(fixture: &Fixture, compile: impl Fn() + Copy) -> SampleSummary {
    let mut durations = Vec::with_capacity(SAMPLE_RUNS);
    let mut peak_heap = 0usize;
    for _ in 0..SAMPLE_RUNS {
        let start = Instant::now();
        let (_, peak) = measure_peak(compile);
        durations.push(start.elapsed());
        peak_heap = peak_heap.max(peak);
    }
    let summary = summarize(durations, peak_heap);
    println!(
        "cast_compile sample fixture={} p50_us={} p95_us={} peak_heap_bytes={}",
        fixture.name,
        summary.p50.as_micros(),
        summary.p95.as_micros(),
        summary.peak_heap_bytes
    );
    summary
}

fn duration_us(duration: Duration) -> u128 {
    duration.as_nanos() / 1_000
}

fn criterion_benchmark(c: &mut Criterion) {
    let fixtures = load_fixtures();

    let mut group = c.benchmark_group("cast_compile_paths");
    group.sample_size(200);
    group.warm_up_time(Duration::from_millis(100));
    group.measurement_time(Duration::from_millis(300));
    for fixture in &fixtures {
        group.bench_with_input(
            BenchmarkId::new("text", &fixture.name),
            fixture,
            |b, fixture| {
                b.iter(|| compile_text(&fixture.text));
            },
        );
        group.bench_with_input(
            BenchmarkId::new("cast", &fixture.name),
            fixture,
            |b, fixture| {
                b.iter(|| compile_cast_json(&fixture.cast_json));
            },
        );
    }
    group.finish();

    println!("fixture,path,p50_us,p95_us,peak_heap_bytes");
    for fixture in &fixtures {
        let text = sample_path(fixture, || {
            compile_text(&fixture.text);
        });
        println!(
            "{},text,{},{},{}",
            fixture.name,
            duration_us(text.p50),
            duration_us(text.p95),
            text.peak_heap_bytes
        );

        let cast = sample_path(fixture, || {
            compile_cast_json(&fixture.cast_json);
        });
        println!(
            "{},cast,{},{},{}",
            fixture.name,
            duration_us(cast.p50),
            duration_us(cast.p95),
            cast.peak_heap_bytes
        );

        let phases = phase_breakdown(&fixture.text);
        println!(
            "{},breakdown,lex_p50_us={},parse_p50_us={},semantic_p50_us={},optimize_p50_us={},compile_p50_us={}",
            fixture.name,
            duration_us(summarize(phases.lex, 0).p50),
            duration_us(summarize(phases.parse, 0).p50),
            duration_us(summarize(phases.semantic, 0).p50),
            duration_us(summarize(phases.optimize, 0).p50),
            duration_us(summarize(phases.compile, 0).p50)
        );

        let _ = &fixture.cast_program;
    }
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
