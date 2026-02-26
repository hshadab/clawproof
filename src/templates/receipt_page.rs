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
    let badge_url = format!("{}/badge/{}", base_url, receipt.id);
    let proof_string = format!("clawproof:{}:{}:{}", receipt.id, receipt.output.label, receipt.status.as_str());

    // OG description
    let og_description = format!(
        "Cryptographically verified ML inference. Model: {}. Result: {} ({:.1}% confidence). Status: {}.",
        receipt.model_name,
        receipt.output.label,
        receipt.output.confidence * 100.0,
        status_label,
    );
    let og_title = format!(
        "ClawProof \u{2014} {} ({:.1}%)",
        receipt.output.label,
        receipt.output.confidence * 100.0,
    );

    // Pre-formatted share texts (escaped for JS strings)
    let verify_me_text = format!(
        "I made this decision: {} ({:.1}% confidence) \u{2014} ML inference cryptographically verified with a @novanet_zkp zkML proof. Don\\'t trust me, verify it: {}",
        receipt.output.label,
        receipt.output.confidence * 100.0,
        receipt_url,
    );

    format!(
        r#"<!DOCTYPE html>
<html lang="en" data-theme="dark">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    {auto_refresh}
    <title>{og_title}</title>

    <!-- OpenGraph -->
    <meta property="og:type" content="article" />
    <meta property="og:title" content="{og_title}" />
    <meta property="og:description" content="{og_description}" />
    <meta property="og:url" content="{receipt_url}" />
    <meta property="og:image" content="{badge_url}" />
    <meta property="og:site_name" content="ClawProof" />

    <!-- Twitter Card -->
    <meta name="twitter:card" content="summary" />
    <meta name="twitter:title" content="{og_title}" />
    <meta name="twitter:description" content="{og_description}" />
    <meta name="twitter:image" content="{badge_url}" />

    <style>
        *, *::before, *::after {{ margin: 0; padding: 0; box-sizing: border-box; }}

        /* Dark palette (default) */
        :root, [data-theme="dark"] {{
            --bg: #0d1117;
            --bg-secondary: #161b22;
            --bg-tertiary: #21262d;
            --border: #30363d;
            --border-light: #21262d;
            --text-primary: #c9d1d9;
            --text-secondary: #8b949e;
            --text-tertiary: #484f58;
            --accent: #f0883e;
            --green: #3fb950;
            --green-bg: rgba(63,185,80,0.1);
            --green-border: rgba(63,185,80,0.3);
            --amber: #d29922;
            --amber-bg: rgba(210,153,34,0.1);
            --amber-border: rgba(210,153,34,0.3);
            --red: #f85149;
            --red-bg: rgba(248,81,73,0.1);
            --red-border: rgba(248,81,73,0.3);
            --link: #58a6ff;
            --mono: 'SF Mono', 'Fira Code', 'JetBrains Mono', Menlo, monospace;
        }}

        /* Light palette */
        [data-theme="light"] {{
            --bg: #ffffff;
            --bg-secondary: #f7f8fa;
            --bg-tertiary: #eef0f4;
            --border: #d8dce3;
            --border-light: #e8ebf0;
            --text-primary: #111827;
            --text-secondary: #4b5563;
            --text-tertiary: #9ca3af;
            --accent: #f0883e;
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
        }}

        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Inter', system-ui, sans-serif;
            background: var(--bg); color: var(--text-primary); min-height: 100vh;
            -webkit-font-smoothing: antialiased;
            transition: background 0.2s, color 0.2s;
        }}

        .page {{ max-width: 600px; margin: 0 auto; padding: 3rem 1.25rem 4rem; }}

        .page-header {{
            display: flex; align-items: center; justify-content: space-between;
            margin-bottom: 1.5rem;
        }}
        .header-left {{
            display: flex; align-items: center; gap: 0.75rem;
        }}
        .wordmark {{
            font-size: 1rem; font-weight: 600; color: var(--text-primary);
            text-decoration: none;
        }}
        .wordmark span {{ color: var(--text-tertiary); font-weight: 400; }}
        .header-right {{
            display: flex; align-items: center; gap: 0.625rem;
        }}
        .theme-toggle {{
            background: var(--bg-secondary); border: 1px solid var(--border);
            border-radius: 6px; padding: 0.25rem 0.5rem; cursor: pointer;
            font-size: 0.875rem; color: var(--text-primary);
            transition: background 0.15s, border-color 0.15s; line-height: 1;
        }}
        .theme-toggle:hover {{ border-color: var(--accent); }}

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
            border-radius: 8px; margin-bottom: 1rem; background: var(--bg-secondary);
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

        /* Proof string */
        .proof-string-bar {{
            display: flex; align-items: center; gap: 0.5rem;
            padding: 0.5rem 0.75rem; border: 1px solid var(--border);
            border-radius: 6px; background: var(--bg-secondary); margin-bottom: 0.75rem;
        }}
        .proof-string-label {{
            font-size: 0.6875rem; font-weight: 600; text-transform: uppercase;
            letter-spacing: 0.05em; color: var(--text-tertiary); flex-shrink: 0;
        }}
        .proof-string-value {{
            font-family: var(--mono); font-size: 0.75rem; color: var(--accent);
            word-break: break-all; flex: 1; min-width: 0;
        }}

        /* Share section */
        .share-section {{ margin-bottom: 1rem; }}
        .share-section-header {{
            font-size: 0.6875rem; font-weight: 600; text-transform: uppercase;
            letter-spacing: 0.05em; color: var(--text-tertiary); margin-bottom: 0.5rem;
        }}
        .share-url-bar {{
            display: flex; align-items: center; justify-content: space-between;
            padding: 0.5rem 0.75rem; border: 1px solid var(--border-light);
            border-radius: 6px; background: var(--bg-secondary); margin-bottom: 0.5rem;
        }}
        .share-url {{
            font-family: var(--mono); font-size: 0.6875rem; color: var(--text-secondary);
            word-break: break-all; min-width: 0;
        }}
        .share-buttons {{
            display: flex; gap: 0.5rem; flex-wrap: wrap;
        }}
        .share-btn {{
            display: inline-flex; align-items: center; gap: 0.375rem;
            padding: 0.375rem 0.625rem; font-size: 0.75rem; font-weight: 500;
            color: var(--text-secondary); background: var(--bg-secondary);
            border: 1px solid var(--border); border-radius: 6px; cursor: pointer;
            transition: background 0.15s, border-color 0.15s, color 0.15s;
            text-decoration: none; line-height: 1.2;
        }}
        .share-btn:hover {{ background: var(--bg-tertiary); border-color: var(--accent); color: var(--text-primary); }}
        .share-btn.primary {{
            background: var(--accent); color: #fff; border-color: var(--accent);
        }}
        .share-btn.primary:hover {{ opacity: 0.9; }}
        .share-btn svg {{ width: 14px; height: 14px; flex-shrink: 0; }}
        .copy-btn {{
            flex-shrink: 0; margin-left: 0.75rem; padding: 0.25rem 0.5rem;
            font-size: 0.6875rem; font-weight: 500; color: var(--text-secondary); background: var(--bg);
            border: 1px solid var(--border); border-radius: 4px; cursor: pointer;
            transition: background 0.15s, border-color 0.15s;
        }}
        .copy-btn:hover {{ background: var(--bg-secondary); border-color: var(--accent); }}

        /* Toast notification */
        .toast {{
            position: fixed; bottom: 1.5rem; left: 50%; transform: translateX(-50%) translateY(100px);
            background: var(--bg-tertiary); color: var(--text-primary); padding: 0.5rem 1rem;
            border-radius: 6px; font-size: 0.8125rem; border: 1px solid var(--border);
            opacity: 0; transition: transform 0.3s ease, opacity 0.3s ease; z-index: 100;
            pointer-events: none;
        }}
        .toast.show {{
            transform: translateX(-50%) translateY(0); opacity: 1;
        }}

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
            <div class="header-left">
                <a class="wordmark" href="/">ClawProof <span>/ receipt</span></a>
            </div>
            <div class="header-right">
                <span class="status-badge {status_class}" aria-label="Proof status: {status_label}">
                    <span class="status-dot"></span>
                    {status_label}
                </span>
                <button class="theme-toggle" id="theme-toggle" onclick="toggleTheme()" title="Toggle dark/light mode"></button>
            </div>
        </div>

        <div class="prediction-card">
            <div class="prediction-label">{label}</div>
            <div class="prediction-confidence">{confidence:.1}% confidence</div>
        </div>

        <div class="proof-string-bar">
            <span class="proof-string-label">Proof ID</span>
            <span class="proof-string-value" id="proof-string">{proof_string}</span>
            <button class="copy-btn" onclick="copyText(document.getElementById('proof-string').textContent, 'Proof string copied')">Copy</button>
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

        <!-- Share section -->
        <div class="share-section">
            <div class="share-section-header">Share this proof</div>
            <div class="share-url-bar">
                <span class="share-url" id="share-url">{receipt_url}</span>
                <button class="copy-btn" onclick="copyText(document.getElementById('share-url').textContent, 'Link copied')">Copy</button>
            </div>
            <div class="share-buttons">
                <a class="share-btn primary" id="x-share-btn" href="https://x.com/intent/tweet?text={x_share_text_encoded}" target="_blank" rel="noopener">
                    <svg viewBox="0 0 24 24" fill="currentColor"><path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z"/></svg>
                    Share on X
                </a>
                <button class="share-btn" onclick="copyVerifyMe()">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/><path d="m9 12 2 2 4-4"/></svg>
                    Copy "Verify me"
                </button>
                <button class="share-btn" onclick="copyText(document.getElementById('proof-string').textContent, 'Proof string copied')">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
                    Copy proof string
                </button>
            </div>
        </div>

        <div class="footer">
            <a href="/">ClawProof</a> &middot;
            <a href="https://github.com/ICME-Lab/jolt-atlas" target="_blank">JOLT-Atlas</a> &middot;
            Open source (MIT)
        </div>
    </div>

    <div class="toast" id="toast"></div>

    <script>
    function initTheme() {{
        var saved = localStorage.getItem('cp-theme');
        var theme = saved || 'dark';
        document.documentElement.setAttribute('data-theme', theme);
        updateToggleIcon(theme);
    }}
    function toggleTheme() {{
        var current = document.documentElement.getAttribute('data-theme');
        var next = current === 'dark' ? 'light' : 'dark';
        document.documentElement.setAttribute('data-theme', next);
        localStorage.setItem('cp-theme', next);
        updateToggleIcon(next);
    }}
    function updateToggleIcon(theme) {{
        document.getElementById('theme-toggle').textContent = theme === 'dark' ? '\u2600' : '\u263E';
    }}

    function copyText(text, msg) {{
        navigator.clipboard.writeText(text).then(function() {{
            showToast(msg || 'Copied');
        }});
    }}

    function copyVerifyMe() {{
        var text = '{verify_me_text}';
        navigator.clipboard.writeText(text).then(function() {{
            showToast('"Verify me" message copied');
        }});
    }}

    function showToast(msg) {{
        var el = document.getElementById('toast');
        el.textContent = msg;
        el.classList.add('show');
        setTimeout(function() {{ el.classList.remove('show'); }}, 2000);
    }}

    initTheme();
    </script>
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
        proof_string = proof_string,
        created_at = receipt.created_at.format("%Y-%m-%d %H:%M:%S UTC"),
        completed_at = completed_at,
        receipt_url = receipt_url,
        badge_url = badge_url,
        og_title = og_title,
        og_description = og_description,
        x_share_text_encoded = urlencoding::encode(&format!(
            "My agent classified this as {} ({:.1}% confidence) \u{2014} ML inference cryptographically verified with a @novanet_zkp zkML proof.\n\nDon't trust me, verify it:\n{}",
            receipt.output.label,
            receipt.output.confidence * 100.0,
            receipt_url,
        )),
        verify_me_text = verify_me_text,
    )
}
