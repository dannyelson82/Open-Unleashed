//! The status page (`GET /`, token-protected): data sets, save profiles with their versions (download and
//! restore), open lobby games and recently seen devices. Plain HTML, no scripts.

use crate::Shared;
use axum::{
    extract::{RawQuery, State},
    response::{Html, IntoResponse, Response},
};
use std::time::{SystemTime, UNIX_EPOCH};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

/// Percent-encoding for one URL path segment.
fn seg(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            o.push(b as char);
        } else {
            o.push_str(&format!("%{b:02X}"));
        }
    }
    o
}

fn mb(n: u64) -> String {
    format!("{:.1} MB", n as f64 / 1e6)
}

fn ago(ms: u64) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    let s = now.saturating_sub(ms) / 1000;
    match s {
        0..=59 => format!("{s} s ago"),
        60..=3599 => format!("{} min ago", s / 60),
        3600..=86399 => format!("{} h ago", s / 3600),
        _ => format!("{} days ago", s / 86400),
    }
}

pub async fn page(State(st): State<Shared>, RawQuery(q): RawQuery) -> Response {
    // keep the token on every link (a browser can't send the Authorization header)
    let tq = q.as_deref().and_then(|q| q.split('&').find(|kv| kv.starts_with("token="))).map(|t| format!("?{t}")).unwrap_or_default();
    let mut h = String::new();
    h.push_str("<!doctype html><html lang=en><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'>");
    h.push_str(&format!("<title>{} — NFS Haven server</title>", esc(&st.cfg.name)));
    h.push_str("<style>body{font:15px/1.45 system-ui,sans-serif;margin:2rem auto;max-width:72rem;padding:0 1rem;color:#1d1d1f;background:#fbfbfd}\
        @media(prefers-color-scheme:dark){body{color:#f5f5f7;background:#111}th{background:#222!important}td,th{border-color:#333!important}}\
        h1{font-size:1.6rem}h2{margin-top:2rem;font-size:1.2rem}table{border-collapse:collapse;width:100%}td,th{border-bottom:1px solid #ddd;padding:.35rem .5rem;text-align:left;vertical-align:top}\
        th{background:#f0f0f3;font-weight:600}.muted{opacity:.65}code{font-size:.9em}form{display:inline}button{font:inherit;padding:.1rem .5rem}</style>");
    h.push_str(&format!("<h1>{}</h1><p class=muted>NFS Haven server {} · data: {} · <a href='/healthz'>healthz</a> · LAN/tailnet only — never port-forward this server.</p>",
        esc(&st.cfg.name), crate::VERSION, esc(&st.data.state_text())));

    // data sets
    h.push_str("<h2>Game data sets</h2><table><tr><th>Game</th><th>Set</th><th>Kind</th><th>Files</th><th>Size</th><th>Manifest</th></tr>");
    {
        let s = st.data.inner.read().unwrap();
        if s.sets.is_empty() {
            h.push_str(&format!("<tr><td colspan=6 class=muted>No data yet. Put each game in its own folder of the games folder ({}): Porsche Unleashed's <code>GameData/</code>, <code>FEData/</code>, <code>version.txt</code>, <code>fe.txt</code> in <code>porsche/original/</code>.</td></tr>", esc(&st.cfg.games_dir.display().to_string())));
        }
        for (id, b) in &s.sets {
            let set = id.split_once('/').map_or(id.as_str(), |(_, s)| s);
            h.push_str(&format!("<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><a href='/v1/data/{}/{}/manifest{tq}'>manifest.json</a></td></tr>",
                esc(&b.game), esc(set), b.kind, b.manifest.files.len(), mb(b.manifest.total_size), seg(&b.game), seg(set)));
        }
    }
    h.push_str("</table>");

    // imports
    imports_section(&mut h, &st, &tq);

    // saves
    h.push_str("<h2>Saves</h2><p class=muted>One section per game (its save namespace).</p>");
    let profiles = st.saves.profiles();
    if profiles.is_empty() {
        h.push_str("<p class=muted>No saves yet.</p>");
    }
    for (p, meta) in profiles {
        h.push_str(&format!("<h3>{}</h3><table><tr><th>File</th><th>Size</th><th>Last write</th><th>By</th><th>Versions</th></tr>", esc(&p)));
        for (f, fm) in &meta.files {
            let versions = st.saves.versions(&p, f);
            let mut vcell = String::new();
            for v in versions.iter().take(10) {
                let base = format!("/v1/saves/{}/version/{}/{}", seg(&p), seg(&v.id), f.split('/').map(seg).collect::<Vec<_>>().join("/"));
                let restore = format!("/v1/saves/{}/restore/{}/{}{tq}", seg(&p), seg(&v.id), f.split('/').map(seg).collect::<Vec<_>>().join("/"));
                vcell.push_str(&format!("<div>{} · {} · <a href='{base}{tq}'>download</a> <form method=post action='{restore}'><button>restore</button></form></div>",
                    ago(v.meta.time), mb(v.meta.size), ));
            }
            if versions.len() > 10 {
                vcell.push_str(&format!("<div class=muted>+ {} older</div>", versions.len() - 10));
            }
            let cur = format!("/v1/saves/{}/files/{}{tq}", seg(&p), f.split('/').map(seg).collect::<Vec<_>>().join("/"));
            h.push_str(&format!("<tr><td><a href='{cur}'><code>{}</code></a></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc(f), mb(fm.size), ago(fm.time), esc(&fm.writer), if vcell.is_empty() { "<span class=muted>none</span>".into() } else { vcell }));
        }
        h.push_str("</table>");
    }

    // lobby
    h.push_str("<h2>Open network games</h2><table><tr><th>Game</th><th>Host</th><th>Track</th><th>Players</th><th>Version</th><th>Addresses</th></tr>");
    let games = st.lobby.list();
    if games.is_empty() {
        h.push_str("<tr><td colspan=6 class=muted>None.</td></tr>");
    }
    for (_, g, _) in games {
        h.push_str(&format!("<tr><td>{}{}</td><td>{}</td><td>{}</td><td>{}/{}</td><td>{}</td><td><code>{}</code></td></tr>",
            esc(&g.name), if g.started { " <span class=muted>(racing)</span>" } else { "" }, esc(&g.host_player), esc(&g.track),
            g.players, g.max_players, esc(&g.version), esc(&g.addresses.join(", "))));
    }
    h.push_str("</table>");

    // devices
    h.push_str("<h2>Recently seen devices</h2><table><tr><th>Device</th><th>Address</th><th>Last request</th><th>When</th></tr>");
    let mut seen: Vec<_> = st.seen.lock().map(|s| s.iter().map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default();
    seen.sort_by(|a, b| b.1.at.cmp(&a.1.at));
    for (k, v) in seen.into_iter().take(20) {
        let ms = v.at.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
        h.push_str(&format!("<tr><td>{}</td><td>{}</td><td><code>{}</code></td><td>{}</td></tr>", esc(&k), esc(&v.addr), esc(&v.last), ago(ms)));
    }
    h.push_str("</table></html>");
    Html(h).into_response()
}

/// The Imports section: the inbox, each game's kept sources (with Re-import), and the recent jobs with their logs.
fn imports_section(h: &mut String, st: &Shared, tq: &str) {
    let im = &st.imports;
    h.push_str("<h2>Imports</h2>");
    if !im.enabled() {
        h.push_str(&format!("<p class=muted>Off: no inbox folder ({}).</p>", esc(&im.inbox.display().to_string())));
        return;
    }
    h.push_str(&format!("<p class=muted>Drop a game's disc image (<code>.img</code>, <code>.bin</code> + <code>.cue</code>, <code>.iso</code>) or an official update into <code>{}</code>; it is recognised by its contents, kept read-only in <code>{}</code> and the game's set is rebuilt (built in <code>{}</code>, verified, then swapped in).</p>",
        esc(&im.inbox.display().to_string()), esc(&im.sources.display().to_string()), esc(&im.staging.display().to_string())));
    let inbox = im.inbox_items();
    h.push_str("<h3>Inbox</h3>");
    if inbox.is_empty() {
        h.push_str("<p class=muted>Empty.</p>");
    } else {
        h.push_str("<table><tr><th>File</th><th>Size</th><th>Status</th></tr>");
        for i in inbox {
            h.push_str(&format!("<tr><td><code>{}</code></td><td>{}</td><td>{}</td></tr>", esc(&i.name), mb(i.size), esc(&i.status)));
        }
        h.push_str("</table>");
    }
    h.push_str("<h3>Sources</h3><table><tr><th>Game</th><th>File</th><th>Kind</th><th>Size</th><th>SHA-256</th><th>Recognised as</th><th></th></tr>");
    let queued = im.queued();
    for (game, list) in im.sources() {
        for (k, s) in list.iter().enumerate() {
            let button = if k == 0 {
                if queued.contains(&game) { "<span class=muted>queued</span>".to_string() } else { format!("<form method=post action='/v1/imports/{}/run{tq}'><button>Re-import</button></form>", seg(&game)) }
            } else {
                String::new()
            };
            let file = match &s.data_file { Some(d) => format!("{} + {}", s.file, d), None => s.file.clone() };
            h.push_str(&format!("<tr><td>{}</td><td><code>{}</code></td><td>{}</td><td>{}</td><td><code>{}…</code></td><td>{}</td><td>{button}</td></tr>",
                esc(&game), esc(&file), esc(&s.kind), mb(s.size), esc(&s.sha256[..16.min(s.sha256.len())]), esc(&s.detail)));
        }
    }
    h.push_str("</table>");
    let jobs = im.jobs();
    h.push_str("<h3>Jobs</h3>");
    if jobs.is_empty() {
        h.push_str("<p class=muted>None yet.</p>");
        return;
    }
    h.push_str("<table><tr><th>When</th><th>Set</th><th>State</th><th>Result</th><th>From</th><th>Log</th></tr>");
    for j in jobs.iter().rev().take(15) {
        let took = j.finished.map(|f| format!(" in {} s", f.saturating_sub(j.started))).unwrap_or_default();
        h.push_str(&format!("<tr><td>{}</td><td>{}/{}</td><td>{}{}</td><td>{}</td><td>{}</td><td><a href='/v1/imports/logs/{}{tq}'>log</a></td></tr>",
            ago(j.started * 1000), esc(&j.game), esc(&j.set), esc(&j.state), took, esc(&j.message), esc(&j.sources.join(", ")), seg(&j.id)));
    }
    h.push_str("</table>");
}
