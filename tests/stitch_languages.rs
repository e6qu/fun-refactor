//! Every language the tool parses, asked whether it can read an environment variable.

use fun_refactor::analysis::stitch;
use fun_refactor::index::Index;
use fun_refactor::scan::ScanOptions;

const CHART: &str = "apiVersion: v2\nname: app\nversion: 0.1.0\n";
const VALUES: &str = "db:\n  url: postgres://localhost/app\n";
const TEMPLATE: &str = "spec:\n  containers:\n    - name: api\n      env:\n        \
                        - name: DATABASE_URL\n          value: {{ .Values.db.url }}\n";

fn chains_over(app: &str, code: &str) -> Vec<stitch::Chain> {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let chart = tmp.path().join("chart/templates");
    std::fs::create_dir_all(&chart).expect("the chart directory");
    std::fs::write(tmp.path().join("chart/Chart.yaml"), CHART).expect("the chart");
    std::fs::write(tmp.path().join("chart/values.yaml"), VALUES).expect("the values");
    std::fs::write(chart.join("d.yaml"), TEMPLATE).expect("the template");
    std::fs::create_dir_all(tmp.path().join("app")).expect("the app directory");
    std::fs::write(tmp.path().join("app").join(app), code).expect("the program");

    let index = Index::build(tmp.path(), &ScanOptions::default()).expect("an index");
    stitch::chains(&index).expect("chains")
}

fn read_once(app: &str, code: &str) -> String {
    let chains = chains_over(app, code);
    let chain = chains
        .iter()
        .find(|c| c.env_var == "DATABASE_URL")
        .expect("the variable is declared");
    assert_eq!(
        chain.reads.len(),
        1,
        "expected exactly one read in {app}, got {:?}",
        chain.reads
    );
    chain.reads[0].text.clone()
}

#[test]
fn java_reads_the_environment() {
    // `System.getenv` is how a Java service reads its configuration.
    let text = read_once(
        "Main.java",
        "public class Main {\n    static String url() { return System.getenv(\"DATABASE_URL\"); }\n}\n",
    );
    assert!(text.contains("System.getenv(\"DATABASE_URL\")"), "{text}");
}

#[test]
fn java_reads_the_environment_through_the_map() {
    let text = read_once(
        "Main.java",
        "public class Main {\n    static String url() { return System.getenv().get(\"DATABASE_URL\"); }\n}\n",
    );
    assert!(text.contains("getenv().get(\"DATABASE_URL\")"), "{text}");
}

#[test]
fn zig_reads_the_environment_past_its_allocator() {
    // Zig's accessor takes the allocator first, so reading the argument straight after the
    // paren found `allocator`, which the upper-case name filter then dropped, and the read went
    // missing without anything saying so.
    let text = read_once(
        "main.zig",
        "const std = @import(\"std\");\npub fn url(a: std.mem.Allocator) ![]u8 {\n    \
         return std.process.getEnvVarOwned(a, \"DATABASE_URL\");\n}\n",
    );
    assert!(
        text.contains("getEnvVarOwned(a, \"DATABASE_URL\")"),
        "{text}"
    );
}

#[test]
fn zig_reads_the_environment_directly() {
    let text = read_once(
        "main.zig",
        "const std = @import(\"std\");\npub fn url() ?[]const u8 {\n    \
         return std.posix.getenv(\"DATABASE_URL\");\n}\n",
    );
    assert!(
        text.contains("std.posix.getenv(\"DATABASE_URL\")"),
        "{text}"
    );
}

#[test]
fn a_language_that_cannot_read_the_environment_is_not_claimed_to() {
    // The capability row asks the analysis instead of repeating its list, which is how
    // that row came to say Java and Zig do not read the environment at all.
    use fun_refactor::lang::Language;
    for language in Language::ALL {
        let claimed = fun_refactor::capabilities::support(
            fun_refactor::capabilities::Capability::Stitch,
            *language,
        )
        .is_yes();
        let declares = matches!(language, Language::Helm | Language::Yaml);
        assert_eq!(
            claimed,
            declares || stitch::reads_environment(*language),
            "the matrix and the analysis disagree about {language}"
        );
    }
}

#[test]
fn a_chart_with_no_metadata_still_starts_its_chain_at_the_values_file() {
    // `svc/chart/templates/d.yaml` full of `{{ .Values.* }}` came through as plain YAML for want of
    // a `Chart.yaml`.
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let chart = tmp.path().join("svc/chart/templates");
    std::fs::create_dir_all(&chart).expect("the chart directory");
    std::fs::write(tmp.path().join("svc/chart/values.yaml"), VALUES).expect("the values");
    std::fs::write(chart.join("d.yaml"), TEMPLATE).expect("the template");
    std::fs::create_dir_all(tmp.path().join("app")).expect("the app directory");
    std::fs::write(
        tmp.path().join("app/main.py"),
        "import os\n\nURL = os.environ[\"DATABASE_URL\"]\n",
    )
    .expect("the program");

    let index = Index::build(tmp.path(), &ScanOptions::default()).expect("an index");
    let chains = stitch::chains(&index).expect("chains");
    let chain = chains
        .iter()
        .find(|c| c.env_var == "DATABASE_URL")
        .expect("the variable is declared");

    assert_eq!(
        chain.values_path.as_deref(),
        Some(["db", "url"].map(String::from).as_slice()),
        "the chain names the key it comes from"
    );
    assert_eq!(
        chain.values_file,
        Some(tmp.path().join("svc/chart/values.yaml")),
        "and the file that holds it"
    );
    assert_eq!(chain.reads.len(), 1, "got {:?}", chain.reads);
}
