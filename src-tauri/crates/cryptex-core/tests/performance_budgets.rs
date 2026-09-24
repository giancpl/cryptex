use cryptex_core::{
    api::{BuildConfiguration, LatexEngine, ResolutionProvenance},
    catalog::{CommandCatalog, CommandContext},
    catalog_search::CatalogSearchQuery,
    compiler::LatexmkRequestBuilder,
    index::{ScannerLimits, project::ProjectIndexer, scanner::scan_latex_file},
    project::ProjectService,
    trust::TrustService,
};
use std::{
    fs,
    hint::black_box,
    time::{Duration, Instant},
};
use tempfile::tempdir;

const TREE_BUDGET: Duration = Duration::from_secs(3);
const INDEX_BUDGET: Duration = Duration::from_secs(20);
const SOURCE_BUDGET: Duration = Duration::from_secs(3);
const FINDER_BUDGET: Duration = Duration::from_secs(3);
const IO_CYCLE_BUDGET: Duration = Duration::from_secs(8);
const ARTIFACT_CYCLE_BUDGET: Duration = Duration::from_secs(8);
const MEMORY_DELTA_BUDGET_KIB: u64 = 256 * 1024;

fn rss_kib() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        line.strip_prefix("VmRSS:")
            .and_then(|value| value.split_whitespace().next())
            .and_then(|value| value.parse().ok())
    })
}

fn assert_budget(label: &str, elapsed: Duration, budget: Duration) {
    println!(
        "{label}: {:.3}s (budget {:.3}s)",
        elapsed.as_secs_f64(),
        budget.as_secs_f64()
    );
    assert!(
        elapsed <= budget,
        "{label} exceeded budget: {elapsed:?} > {budget:?}"
    );
}

#[test]
#[ignore = "scheduled performance gate; run through pnpm performance:test"]
fn performance_budgets_are_met() {
    let tree = tempdir().expect("large project");
    for index in 0..10_000 {
        fs::write(
            tree.path().join(format!("file-{index:05}.tex")),
            format!("\\section{{Section {index}}}\\label{{s:{index}}}\n"),
        )
        .expect("fixture source");
    }

    let mut projects = ProjectService::default();
    let summary = projects.open(tree.path()).expect("open project");
    let started = Instant::now();
    let page = projects
        .list_directory(&summary.project_id, "")
        .expect("list tree");
    let elapsed = started.elapsed();
    assert!(page.truncated);
    assert_eq!(page.entries.len(), 5_000);
    assert_budget("10k-file lazy tree", elapsed, TREE_BUDGET);

    let started = Instant::now();
    let indexer = ProjectIndexer::open("performance".into(), tree.path(), ScannerLimits::default())
        .expect("index large project");
    let elapsed = started.elapsed();
    assert_eq!(indexer.snapshot().files.len(), 10_000);
    assert_budget("10k-file project index", elapsed, INDEX_BUDGET);

    let source = "x".repeat(5 * 1024 * 1024);
    let rss_before = rss_kib();
    let started = Instant::now();
    let scanned = scan_latex_file(
        "large.tex",
        &"a".repeat(64),
        &source,
        ScannerLimits::default(),
    )
    .expect("scan 5 MiB source");
    black_box(scanned);
    let elapsed = started.elapsed();
    assert_budget("5 MiB tolerant scan", elapsed, SOURCE_BUDGET);
    if let (Some(before), Some(after)) = (rss_before, rss_kib()) {
        let delta = after.saturating_sub(before);
        println!("scanner RSS delta: {delta} KiB (budget {MEMORY_DELTA_BUDGET_KIB} KiB)");
        assert!(
            delta <= MEMORY_DELTA_BUDGET_KIB,
            "scanner RSS delta exceeded budget"
        );
    }

    let catalog = CommandCatalog::bundled().expect("catalog");
    let snapshot = indexer.snapshot();
    let queries = ["sample", "pseudocode", "reference", "cipher", "section"];
    let started = Instant::now();
    for iteration in 0..5_000 {
        let request = CatalogSearchQuery::from_project_index(
            queries[iteration % queries.len()].into(),
            Some(CommandContext::Math),
            20,
            &snapshot,
        );
        black_box(catalog.search(&request).expect("catalog search"));
    }
    assert_budget("5k Finder queries", started.elapsed(), FINDER_BUDGET);

    let io = tempdir().expect("I/O cycle project");
    fs::write(io.path().join("main.tex"), "0").expect("source");
    let mut service = ProjectService::default();
    let project = service.open(io.path()).expect("open I/O project");
    let mut document = service
        .read_text_file(&project.project_id, "main.tex")
        .expect("initial read");
    let started = Instant::now();
    for revision in 1..=100 {
        service
            .write_text_file(
                &project.project_id,
                "main.tex",
                &revision.to_string(),
                &document.fingerprint,
            )
            .expect("atomic cycle write");
        document = service
            .read_text_file(&project.project_id, "main.tex")
            .expect("cycle read");
    }
    assert_eq!(document.text, "100");
    assert_budget(
        "100 atomic read/write cycles",
        started.elapsed(),
        IO_CYCLE_BUDGET,
    );

    let project = tempdir().expect("artifact project");
    fs::write(
        project.path().join("main.tex"),
        "\\documentclass{article}\n",
    )
    .expect("root");
    let state = tempdir().expect("artifact state");
    let mut projects = ProjectService::default();
    let summary = projects
        .open(project.path())
        .expect("open artifact project");
    let trust = TrustService::load(state.path().join("trust.json")).expect("trust");
    let builder = LatexmkRequestBuilder::new(state.path().join("builds")).expect("builder");
    let request = builder
        .build(
            &projects,
            &trust,
            BuildConfiguration {
                api_version: 1,
                project_id: summary.project_id.clone(),
                root_document: "main.tex".into(),
                root_provenance: ResolutionProvenance::Detected,
                engine: LatexEngine::PdfLatex,
                engine_provenance: ResolutionProvenance::Default,
            },
        )
        .expect("build request");
    let started = Instant::now();
    for revision in 1..=100 {
        fs::write(
            &request.artifacts.pdf,
            format!("%PDF-1.7\nrevision {revision}\n%%EOF\n"),
        )
        .expect("build PDF");
        let operation_id = format!("build-{revision:020}");
        let retained = builder
            .retain_pdf_artifact(
                &request.artifacts.pdf,
                &summary.project_id,
                &operation_id,
                1024,
            )
            .expect("retain PDF");
        black_box(
            builder
                .read_bounded_artifact(&retained, 1024)
                .expect("view PDF"),
        );
    }
    assert_budget(
        "100 build/view artifact cycles",
        started.elapsed(),
        ARTIFACT_CYCLE_BUDGET,
    );
}
