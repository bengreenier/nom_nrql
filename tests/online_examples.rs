//! Try parsing NRQL examples from New Relic docs and other sources.
//! Logs which ones fail so we can identify parser gaps.

use nom_nrql::parse_nrql;

/// Examples from https://docs.newrelic.com/docs/nrql/nrql-examples/app-data-nrql-query-examples/
/// and related docs. Queries that we expect to parse successfully.
const EXAMPLES_TO_TRY: &[(&str, &str)] = &[
    ("unique users", "SELECT uniqueCount(session) FROM PageView SINCE 1 week ago"),
    ("unique user trends", "SELECT uniqueCount(session) FROM PageView SINCE 1 week ago COMPARE WITH 1 week ago"),
    ("pageview trends", "SELECT count(*) FROM PageView SINCE 1 day ago COMPARE WITH 1 day ago TIMESERIES AUTO"),
    ("OS version facet", "SELECT uniqueCount(uuid) FROM MobileSession FACET osVersion SINCE 7 days ago"),
    ("where string", "SELECT count(*) FROM Transaction WHERE customerName = 'ReallyImportantCustomer' SINCE 1 day ago"),
    ("latest duration", "SELECT latest(duration) FROM Public_APICall WHERE awsAPI = 'sqs' SINCE 1 day ago"),
    ("uniqueCount attr", "SELECT uniqueCount(http.url) FROM Public_APICall SINCE 1 day ago"),
    ("average duration", "SELECT average(duration) FROM Transaction SINCE 12 hours ago"),
    ("percentile", "SELECT percentile(duration, 95) FROM Transaction SINCE 1 day ago"),
    ("facet buckets numeric", "SELECT average(duration) FROM Transaction FACET buckets(duration, 400, 10) SINCE 12 hours ago"),
    ("FROM first", "FROM Transaction SELECT count(*) SINCE 1 hour ago"),
    ("minimal", "SELECT * FROM Transaction"),
    ("limit offset", "FROM Log SELECT message LIMIT 100 OFFSET 50 SINCE 1 day ago"),
    ("order by facet", "SELECT count(*) FROM Transaction FACET appName ORDER BY count(*) DESC LIMIT 5 SINCE 1 week ago"),
    ("with timezone", "SELECT count(*) FROM Transaction WITH TIMEZONE 'America/New_York' SINCE 1 day ago"),
    ("since now", "FROM Transaction SELECT * SINCE NOW"),
    ("backtick attr", "SELECT `user.id`, name FROM Transaction SINCE 1 day ago"),
    ("where in", "SELECT * FROM Log WHERE level IN ('ERROR', 'WARN') SINCE 1 day ago"),
    ("where numeric", "SELECT * FROM Transaction WHERE duration > 1000 SINCE 1 day ago"),
    ("apdex two args", "SELECT apdex(duration, 0.4) FROM Transaction SINCE 1 day ago"),
    ("multiple event types", "FROM Transaction, PageView SELECT count(*) SINCE 3 days ago"),
    ("apdex named arg", "SELECT apdex(duration, t: 0.4) FROM Transaction WHERE customerName = 'x' SINCE 1 day ago"),
    ("filter with WHERE", "SELECT filter(count(*), WHERE awsAPI = 'dynamodb') AS 'DynamoDB' FROM Public_APICall SINCE 1 day ago"),
    ("rate with interval", "SELECT rate(count(*), 5 minutes) FROM Public_APICall SINCE 1 hour ago"),
    ("funnel with WHERE", "SELECT funnel(awsAPI, WHERE http.url LIKE '%.amazonaws.com', WHERE http.url LIKE '%.us-west%') FROM Public_APICall SINCE 1 week ago"),
    ("FACET CASES", "SELECT count(*) FROM Public_APICall FACET CASES(WHERE http.url LIKE '%amazon%', WHERE http.url LIKE '%google%') SINCE 1 day ago"),
    ("EXTRAPOLATE", "SELECT count(*) FROM Transaction SINCE 60 minutes ago FACET appName TIMESERIES 1 minute EXTRAPOLATE"),
];

/// 20 more examples from New Relic intro/process-your-data tutorials and docs.
const EXAMPLES_TO_TRY_2: &[(&str, &str)] = &[
    ("limit 1", "SELECT * FROM Transaction LIMIT 1"),
    ("select name duration", "SELECT name, duration FROM Transaction"),
    ("max duration", "SELECT max(duration) FROM Transaction"),
    ("min duration", "SELECT min(duration) FROM Transaction"),
    ("sum attr", "SELECT sum(databaseCallCount) FROM Transaction"),
    ("avg since 1 day", "SELECT average(duration) FROM Transaction SINCE 1 day ago"),
    ("since until", "SELECT average(duration) FROM Transaction SINCE 1 week ago UNTIL 2 days ago"),
    ("timeseries auto", "SELECT average(duration) FROM Transaction SINCE 1 day ago TIMESERIES AUTO"),
    ("timeseries 1 hour", "SELECT average(duration) FROM Transaction SINCE 1 day ago TIMESERIES 1 hour"),
    ("where web timeseries", "SELECT average(duration) FROM Transaction WHERE transactionType = 'Web' TIMESERIES AUTO"),
    ("facet name", "SELECT average(duration) FROM Transaction FACET name SINCE 1 day ago"),
    ("facet limit timeseries", "SELECT average(duration) FROM Transaction FACET name SINCE 3 hours ago LIMIT 5 TIMESERIES AUTO"),
    ("facet limit 20", "SELECT average(duration) FROM Transaction FACET name SINCE 3 hours ago LIMIT 20"),
    ("where facet limit timeseries", "SELECT count(*) FROM Transaction WHERE transactionType = 'Web' FACET appName LIMIT 5 SINCE 6 hours ago TIMESERIES AUTO"),
    ("uniques", "SELECT uniques(http.url) FROM Public_APICall SINCE 1 day ago"),
    ("earliest", "SELECT earliest(duration) FROM Public_APICall WHERE awsAPI = 'sqs' SINCE 1 day ago"),
    ("percentile 98", "SELECT percentile(duration, 98) FROM Public_APICall SINCE 1 day ago"),
    ("where like facet", "SELECT count(*) FROM Public_APICall WHERE http.url LIKE '%amazonaws%' FACET http.url SINCE 1 day ago"),
    ("where not like", "SELECT count(*) FROM Public_APICall WHERE http.url NOT LIKE '%google%' FACET http.url SINCE 1 day ago"),
    ("compare with timeseries", "SELECT average(duration) FROM Public_APICall SINCE 1 day ago COMPARE WITH 1 week ago TIMESERIES AUTO"),
];

/// 20 more unique examples from advanced docs, different event types, and variant clause combinations.
const EXAMPLES_TO_TRY_3: &[(&str, &str)] = &[
    ("median duration", "SELECT median(duration) FROM Transaction SINCE 1 day ago"),
    ("stddev duration", "SELECT stddev(duration) FROM Transaction SINCE 24 hours ago"),
    ("histogram three args", "SELECT histogram(duration, 1, 20) FROM Public_APICall SINCE 1 day ago"),
    ("from log level message", "FROM Log SELECT level, message LIMIT 50 SINCE 2 hours ago"),
    ("two event types NrdbQuery", "SELECT count(*) FROM NrdbQuery, NrDailyUsage SINCE 1 day ago"),
    ("where response code", "SELECT average(duration) FROM Transaction WHERE httpResponseCode = 200 SINCE 1 day ago"),
    ("where duration float", "SELECT count(*) FROM Transaction WHERE duration < 0.5 SINCE 1 hour ago"),
    ("from metric sum", "FROM Metric SELECT sum(requests) SINCE 30 minutes ago"),
    ("facet countryCode", "SELECT uniqueCount(session) FROM PageView FACET countryCode SINCE 1 week ago"),
    ("three attrs limit", "SELECT name, duration, error FROM Transaction LIMIT 10 SINCE 1 day ago"),
    ("facet two attrs", "SELECT count(*) FROM Transaction FACET appName, name LIMIT 5 SINCE 6 hours ago"),
    ("where is null", "SELECT count(*) FROM Transaction WHERE error IS NULL SINCE 1 day ago"),
    ("since until weeks", "SELECT average(duration) FROM Transaction SINCE 2 weeks ago UNTIL 1 week ago"),
    ("from Span", "FROM Span SELECT count(*) SINCE 1 day ago"),
    ("since 30 minutes", "SELECT count(*) FROM Transaction SINCE 30 minutes ago"),
    ("compare with 6 hours", "SELECT average(duration) FROM Public_APICall SINCE 1 hour ago COMPARE WITH 6 hours ago"),
    ("apdex with alias", "SELECT apdex(duration, 0.1) AS 'Apdex Of Duration' FROM Public_APICall SINCE 1 week ago"),
    ("where like pattern", "SELECT count(*) FROM Transaction WHERE name LIKE '%Controller%' SINCE 1 day ago"),
    ("facet order asc", "SELECT count(*) FROM Transaction FACET appName ORDER BY appName ASC LIMIT 10 SINCE 1 day ago"),
    ("timeseries 5 minutes", "SELECT average(duration) FROM Transaction SINCE 1 day ago TIMESERIES 5 minutes"),
];

/// Examples that use syntax we don't support yet — parse expected to fail.
const EXPECTED_FAILURES: &[(&str, &str)] = &[];

#[test]
fn try_online_examples() {
    let mut failed = Vec::new();
    for (name, query) in EXAMPLES_TO_TRY
        .iter()
        .chain(EXAMPLES_TO_TRY_2.iter())
        .chain(EXAMPLES_TO_TRY_3.iter())
    {
        match parse_nrql(query) {
            Ok(_) => {}
            Err(e) => failed.push((*name, *query, e.message.clone())),
        }
    }
    for (name, query) in EXPECTED_FAILURES {
        if let Ok(_) = parse_nrql(query) {
            failed.push((*name, *query, "expected parse failure but succeeded".to_string()));
        }
    }
    if !failed.is_empty() {
        eprintln!("{} query(ies) had issues:\n", failed.len());
        for (name, query, msg) in &failed {
            eprintln!("  [{}] {}", name, query);
            eprintln!("       -> {}\n", msg);
        }
        panic!("{} example(s) failed (see stderr)", failed.len());
    }
}
