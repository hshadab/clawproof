use crate::receipt::{Receipt, ReceiptStatus};

pub fn render(receipt: &Receipt, base_url: &str) -> String {
    let status_class = match receipt.status {
        ReceiptStatus::Proving => "proving",
        ReceiptStatus::Verified => "verified",
        ReceiptStatus::Failed => "failed",
    };

    let status_label = match receipt.status {
        ReceiptStatus::Proving => "Proving",
        ReceiptStatus::Verified => "Verified",
        ReceiptStatus::Failed => "Failed",
    };

    let auto_refresh = if receipt.status == ReceiptStatus::Proving {
        r#"<meta http-equiv="refresh" content="3">"#
    } else {
        ""
    };

    let proof_section = match receipt.status {
        ReceiptStatus::Verified => {
            format!(
                r#"<div class="card">
                    <div class="card-header">Proof</div>
                    <div class="row"><span class="row-label">Proof hash</span><span class="row-value mono">{}</span></div>
                    <div class="row"><span class="row-label">Size</span><span class="row-value">{} bytes</span></div>
                    <div class="row"><span class="row-label">Prove time</span><span class="row-value">{} ms</span></div>
                    <div class="row last"><span class="row-label">Verify time</span><span class="row-value">{} ms</span></div>
                </div>"#,
                receipt.proof_hash.as_deref().unwrap_or("\u{2014}"),
                receipt.proof_size.map(|s| s.to_string()).unwrap_or_else(|| "\u{2014}".to_string()),
                receipt.prove_time_ms.map(|t| t.to_string()).unwrap_or_else(|| "\u{2014}".to_string()),
                receipt.verify_time_ms.map(|t| t.to_string()).unwrap_or_else(|| "\u{2014}".to_string()),
            )
        }
        ReceiptStatus::Proving => {
            r#"<div class="card">
                <div class="card-header">Proof</div>
                <div class="proving-notice" role="status">
                    <div class="spinner"></div>
                    <span>Generating SNARK proof. This page refreshes automatically.</span>
                </div>
            </div>"#
                .to_string()
        }
        ReceiptStatus::Failed => {
            format!(
                r#"<div class="card">
                    <div class="card-header">Error</div>
                    <div class="error-notice">{}</div>
                </div>"#,
                receipt.error.as_deref().unwrap_or("Unknown error")
            )
        }
    };

    let completed_at = receipt
        .completed_at
        .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "\u{2014}".to_string());

    let receipt_url = format!("{}/receipt/{}", base_url, receipt.id);

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    {auto_refresh}
    <title>Receipt — clawproof</title>
    <style>
        *, *::before, *::after {{ margin: 0; padding: 0; box-sizing: border-box; }}

        :root {{
            --bg: #ffffff;
            --bg-secondary: #f7f8fa;
            --border: #d8dce3;
            --border-light: #e8ebf0;
            --text-primary: #111827;
            --text-secondary: #4b5563;
            --text-tertiary: #9ca3af;
            --green: #16a34a;
            --green-bg: #f0fdf4;
            --green-border: #bbf7d0;
            --amber: #d97706;
            --amber-bg: #fffbeb;
            --amber-border: #fde68a;
            --red: #dc2626;
            --red-bg: #fef2f2;
            --red-border: #fecaca;
            --link: #2563eb;
            --mono: 'SF Mono', 'Fira Code', 'JetBrains Mono', 'Cascadia Code', Menlo, monospace;
        }}

        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Inter', system-ui, sans-serif;
            background: var(--bg); color: var(--text-primary); min-height: 100vh;
            -webkit-font-smoothing: antialiased;
        }}

        .page {{ max-width: 600px; margin: 0 auto; padding: 3rem 1.25rem 4rem; }}

        .page-header {{
            display: flex; align-items: center; justify-content: space-between;
            margin-bottom: 1.5rem;
        }}
        .wordmark {{
            font-size: 1rem; font-weight: 600; color: var(--text-primary);
            text-decoration: none;
        }}
        .wordmark span {{ color: var(--text-tertiary); font-weight: 400; }}

        .status-badge {{
            display: inline-flex; align-items: center; gap: 0.375rem;
            padding: 0.25rem 0.625rem; border-radius: 9999px; font-size: 0.75rem; font-weight: 500;
        }}
        .status-badge.proving {{ background: var(--amber-bg); color: var(--amber); border: 1px solid var(--amber-border); }}
        .status-badge.verified {{ background: var(--green-bg); color: var(--green); border: 1px solid var(--green-border); }}
        .status-badge.failed {{ background: var(--red-bg); color: var(--red); border: 1px solid var(--red-border); }}
        .status-dot {{
            width: 5px; height: 5px; border-radius: 50%; background: currentColor;
        }}
        .status-badge.proving .status-dot {{ animation: pulse 1.5s ease-in-out infinite; }}
        @keyframes pulse {{ 0%, 100% {{ opacity: 1; }} 50% {{ opacity: 0.35; }} }}

        /* Prediction hero */
        .prediction-card {{
            text-align: center; padding: 1.5rem 1rem; border: 1px solid var(--border);
            border-radius: 8px; margin-bottom: 1rem;
        }}
        .prediction-label {{
            font-size: 1.375rem; font-weight: 600; color: var(--text-primary); letter-spacing: -0.01em;
        }}
        .prediction-confidence {{
            color: var(--text-tertiary); font-size: 0.8125rem; margin-top: 0.125rem;
        }}

        /* Cards */
        .card {{
            border: 1px solid var(--border); border-radius: 8px; margin-bottom: 1rem; overflow: hidden;
        }}
        .card-header {{
            font-size: 0.6875rem; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em;
            color: var(--text-tertiary); padding: 0.75rem 1rem 0.5rem; background: var(--bg-secondary);
            border-bottom: 1px solid var(--border-light);
        }}
        .row {{
            display: flex; justify-content: space-between; align-items: flex-start;
            padding: 0.5rem 1rem; border-bottom: 1px solid var(--border-light); gap: 1rem;
        }}
        .row.last {{ border-bottom: none; }}
        .row-label {{ font-size: 0.8125rem; color: var(--text-tertiary); flex-shrink: 0; white-space: nowrap; }}
        .row-value {{
            font-size: 0.8125rem; color: var(--text-primary); text-align: right; word-break: break-all;
            min-width: 0;
        }}
        .mono {{ font-family: var(--mono); font-size: 0.75rem; }}

        /* Proving notice */
        .proving-notice {{
            display: flex; align-items: center; gap: 0.625rem; padding: 0.875rem 1rem;
            color: var(--text-secondary); font-size: 0.8125rem;
        }}
        .spinner {{
            width: 16px; height: 16px; border: 2px solid var(--border); border-top-color: var(--amber);
            border-radius: 50%; animation: spin 0.8s linear infinite; flex-shrink: 0;
        }}
        @keyframes spin {{ to {{ transform: rotate(360deg); }} }}

        .error-notice {{
            padding: 0.875rem 1rem; color: var(--red); font-size: 0.8125rem;
        }}

        /* Share link */
        .share-bar {{
            display: flex; align-items: center; justify-content: space-between;
            padding: 0.625rem 0.75rem; border: 1px solid var(--border-light);
            border-radius: 6px; background: var(--bg-secondary); margin-bottom: 1rem;
        }}
        .share-url {{
            font-family: var(--mono); font-size: 0.6875rem; color: var(--text-secondary);
            word-break: break-all; min-width: 0;
        }}
        .copy-btn {{
            flex-shrink: 0; margin-left: 0.75rem; padding: 0.25rem 0.5rem;
            font-size: 0.6875rem; font-weight: 500; color: var(--text-secondary); background: var(--bg);
            border: 1px solid var(--border); border-radius: 4px; cursor: pointer;
            transition: background 0.15s;
        }}
        .copy-btn:hover {{ background: var(--bg-secondary); }}

        .footer {{
            text-align: center; margin-top: 2.5rem; padding-top: 1.25rem;
            border-top: 1px solid var(--border-light); color: var(--text-tertiary); font-size: 0.75rem;
        }}
        .footer a {{ color: var(--text-tertiary); text-decoration: none; }}
        .footer a:hover {{ color: var(--text-secondary); }}
    </style>
</head>
<body>
    <div class="page">
        <div class="page-header">
            <a class="wordmark" href="/">clawproof <span>/ receipt</span></a>
            <span class="status-badge {status_class}" aria-label="Proof status: {status_label}">
                <span class="status-dot"></span>
                {status_label}
            </span>
        </div>

        <div class="prediction-card">
            <div class="prediction-label">{label}</div>
            <div class="prediction-confidence">{confidence:.1}% confidence</div>
        </div>

        <div class="card">
            <div class="card-header">Model</div>
            <div class="row"><span class="row-label">Name</span><span class="row-value">{model_name}</span></div>
            <div class="row"><span class="row-label">ID</span><span class="row-value mono">{model_id}</span></div>
            <div class="row last"><span class="row-label">Hash</span><span class="row-value mono">{model_hash}</span></div>
        </div>

        <div class="card">
            <div class="card-header">Hashes</div>
            <div class="row"><span class="row-label">Input</span><span class="row-value mono">{input_hash}</span></div>
            <div class="row last"><span class="row-label">Output</span><span class="row-value mono">{output_hash}</span></div>
        </div>

        {proof_section}

        <div class="card">
            <div class="card-header">Metadata</div>
            <div class="row"><span class="row-label">Receipt ID</span><span class="row-value mono">{receipt_id}</span></div>
            <div class="row"><span class="row-label">Created</span><span class="row-value">{created_at}</span></div>
            <div class="row last"><span class="row-label">Completed</span><span class="row-value">{completed_at}</span></div>
        </div>

        <div class="share-bar">
            <span class="share-url" id="share-url">{receipt_url}</span>
            <button class="copy-btn" onclick="navigator.clipboard.writeText(document.getElementById('share-url').textContent)">Copy</button>
        </div>

        <div class="footer">
            <a href="https://github.com/ICME-Lab/jolt-atlas" target="_blank">JOLT-Atlas</a>
        </div>
    </div>
</body>
</html>"#,
        auto_refresh = auto_refresh,
        receipt_id = receipt.id,
        status_class = status_class,
        status_label = status_label,
        label = receipt.output.label,
        confidence = receipt.output.confidence * 100.0,
        model_name = receipt.model_name,
        model_id = receipt.model_id,
        model_hash = receipt.model_hash,
        input_hash = receipt.input_hash,
        output_hash = receipt.output_hash,
        proof_section = proof_section,
        created_at = receipt.created_at.format("%Y-%m-%d %H:%M:%S UTC"),
        completed_at = completed_at,
        receipt_url = receipt_url,
    )
}
