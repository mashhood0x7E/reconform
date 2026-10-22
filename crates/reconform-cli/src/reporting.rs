use crate::{Error, Options, emit, model::VERSION, read};
use serde_json::Value;

fn escape(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            '\'' => "&apos;".into(),
            c if c == '\n'
                || c == '\r'
                || c == '\t'
                || (c >= '\u{20}' && c != '\u{fffe}' && c != '\u{ffff}') =>
            {
                c.to_string()
            }
            _ => "�".into(),
        })
        .collect()
}

pub fn junit(report: &Value) -> Result<String, Error> {
    let fallback = vec![
        serde_json::json!({"id":"capture","verdict":report["verdict"],"code":report["code"],"required":true}),
    ];
    let findings = report["findings"].as_array().unwrap_or(&fallback);
    if findings.is_empty() || findings.len() > 1024 {
        return Err(Error::config("JUnit requires 1..1024 findings"));
    }
    let failures = findings
        .iter()
        .filter(|f| f["verdict"] == "fail" && f["required"] == true)
        .count();
    let errors = findings
        .iter()
        .filter(|f| f["verdict"] == "error" && f["required"] == true)
        .count();
    let skipped = findings
        .iter()
        .filter(|f| {
            !matches!(f["verdict"].as_str(), Some("pass" | "fail" | "error"))
                || (f["required"] == false && f["verdict"] != "pass")
        })
        .count();
    let name = escape(report["scenario"].as_str().unwrap_or("capture"));
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuite name=\"{name}\" tests=\"{}\" failures=\"{failures}\" errors=\"{errors}\" skipped=\"{skipped}\">\n",
        findings.len()
    );
    for f in findings {
        let id = escape(
            f["id"]
                .as_str()
                .ok_or_else(|| Error::config("finding id missing"))?,
        );
        let code = escape(f["code"].as_str().unwrap_or("CAPTURE_INCOMPLETE"));
        xml.push_str(&format!("  <testcase name=\"{id}\">"));
        let element = match f["verdict"].as_str() {
            Some("pass") => None,
            _ if f["required"] == false => Some("skipped"),
            Some("fail") => Some("failure"),
            Some("error") => Some("error"),
            _ => Some("skipped"),
        };
        if let Some(element) = element {
            xml.push_str(&format!("<{element} message=\"{code}\"/>"));
        }
        xml.push_str("</testcase>\n");
    }
    xml.push_str("</testsuite>\n");
    Ok(xml)
}

pub fn execute(opts: &Options) -> Result<u8, Error> {
    if opts.positional.len() != 1 || opts.plan.is_some() || opts.kind.is_some() {
        return Err(Error::config(
            "report requires REPORT [--format human|json|junit]",
        ));
    }
    let (value, _) = read::<Value>(&opts.positional[0])?;
    if value["schema_version"] != VERSION
        || !matches!(
            value["method"].as_str(),
            Some("recorded-parity/1" | "recorded-contracts/1")
        )
    {
        return Err(Error::config("unsupported saved report method or schema"));
    }
    let findings: Vec<crate::compare::Finding> = serde_json::from_value(value["findings"].clone())
        .map_err(|_| Error::config("invalid saved findings"))?;
    if findings.is_empty() || findings.len() > 1024 {
        return Err(Error::config("invalid saved finding count"));
    }
    let mut checks = Vec::new();
    for finding in &findings {
        let verdict = match finding.verdict.as_str() {
            "pass" => reconform_core::Verdict::Pass,
            "fail" => reconform_core::Verdict::Fail,
            "error" => reconform_core::Verdict::Error,
            "inconclusive" => reconform_core::Verdict::Inconclusive,
            "unsupported" => reconform_core::Verdict::Unsupported,
            _ => return Err(Error::config("invalid saved finding verdict")),
        };
        checks.push(reconform_core::Check {
            required: finding.required,
            verdict,
        });
    }
    if value["verdict"] != reconform_core::aggregate(checks).as_str() {
        return Err(Error::config("saved verdict contradicts required findings"));
    }
    let verdict = value["verdict"]
        .as_str()
        .ok_or_else(|| Error::config("saved verdict missing"))?;
    let exit = match verdict {
        "pass" => 0,
        "fail" => 1,
        "error" => 3,
        "inconclusive" => 4,
        _ => return Err(Error::config("invalid saved verdict")),
    };
    emit(
        &value,
        &format!("Saved report: {verdict} ({} findings)\n", findings.len()),
        opts,
    )?;
    Ok(exit)
}
