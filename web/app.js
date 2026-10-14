/* CodeBase app. The project's design, its end-to-end flows and its history, in plain words,
   plus context packs for AI chat apps. Data comes from the local server (src/main.rs). */
(() => {
  'use strict';

  // ---------- helpers ----------
  const $ = (s, root = document) => root.querySelector(s);
  const $$ = (s, root = document) => [...root.querySelectorAll(s)];
  const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  const plural = (n, one, many) => `${n} ${n === 1 ? one : many || one + 's'}`;
  const tokens = (s) => Math.ceil((s || '').length / 4);
  const cap = (s) => (s ? s.charAt(0).toUpperCase() + s.slice(1) : s);
  const idOf = (end) => (typeof end === 'object' ? end.id : end);
  const base = (p) => String(p || '').split('/').pop();
  const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;

  async function api(url, body) {
    const r = await fetch(url, body !== undefined ? { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) } : {});
    const data = await r.json().catch(() => ({}));
    if (!r.ok) throw new Error(data.error || `Request failed (${r.status})`);
    return data;
  }
  let toastTimer;
  function toast(msg) { const t = $('#toast'); t.textContent = msg; t.hidden = false; clearTimeout(toastTimer); toastTimer = setTimeout(() => { t.hidden = true; }, 2600); }
  async function copyText(text) {
    try { await navigator.clipboard.writeText(text); return true; }
    catch { const ta = document.createElement('textarea'); ta.value = text; document.body.appendChild(ta); ta.select(); const ok = document.execCommand('copy'); ta.remove(); return ok; }
  }
  function joinList(parts) {
    if (parts.length <= 1) return parts.join('');
    return parts.slice(0, -1).join(', ') + ' and ' + parts[parts.length - 1];
  }
  function ago(ms) {
    const s = Math.max(1, Math.round((Date.now() - ms) / 1000));
    if (s < 60) return 'just now';
    const m = Math.round(s / 60); if (m < 60) return `${m} min ago`;
    const h = Math.round(m / 60); if (h < 24) return `${h} h ago`;
    const d = Math.round(h / 24); if (d < 30) return plural(d, 'day') + ' ago';
    return new Date(ms).toLocaleDateString();
  }

  // ---------- families: where a part sits on the data path ----------
  const FAMILY = {
    entry: 'people', screens: 'people', components: 'people', state: 'people', views: 'people', styles: 'people', cli: 'people', scenarios: 'people', testcases: 'people', notebooks: 'people',
    apiclient: 'request', routes: 'request', middleware: 'request', controllers: 'request', validation: 'request', steps: 'request', hooks: 'request',
    services: 'logic', jobs: 'logic', integrations: 'logic', pages: 'logic', driver: 'logic', ml_model: 'logic', ml_train: 'logic', ml_eval: 'logic',
    repos: 'data', models: 'data', db: 'data', testdata: 'data', ml_data: 'data',
    app_under_test: 'outside', server: 'outside',
    utils: 'support', config: 'support', infra: 'support', docs: 'support', assets: 'support', testutil: 'support', testconfig: 'support', reports: 'support',
  };
  const FAMILY_NAMES = {
    people: 'What people see or write', request: 'Carries the request', logic: 'Does the work', data: 'Holds the data', outside: 'Outside this project', support: 'Helpers and setup',
  };
  const famOf = (comp) => FAMILY[comp] || 'support';
  const famVar = (comp) => `var(--f-${famOf(comp)})`;
  let famHex = {};
  function readColors() {
    const cs = getComputedStyle(document.documentElement);
    for (const f of Object.keys(FAMILY_NAMES)) famHex[f] = cs.getPropertyValue(`--f-${f}`).trim() || '#888';
    famHex.bg = cs.getPropertyValue('--bg').trim() || '#15181d';
    famHex.line = cs.getPropertyValue('--line').trim() || '#313844';
    famHex.text = cs.getPropertyValue('--text').trim() || '#e6e9ed';
    famHex.text3 = cs.getPropertyValue('--text-3').trim() || '#6e7682';
    famHex.accent = cs.getPropertyValue('--accent').trim() || '#5b8def';
  }

  const KIND_LABEL = { scenario: 'Scenario', maestro: 'Maestro flow', test: 'Test', request: 'Request', screen: 'Screen' };
  const KIND_SHORT = (f) => (f.kind === 'request' ? (f.method || 'REQ') : f.kind === 'screen' ? 'Screen' : f.kind === 'scenario' ? 'Test' : f.kind === 'maestro' ? 'Flow' : 'Test');

  // ---------- state ----------
  const S = {
    meta: null, byId: new Map(), links: [], project: null, version: 0, view: 'overview',
    arch: { components: [], pipeline: [], edges: [], sides: [], tree: null, path: [] },
    flows: [], journeys: [], checks: [], flowById: new Map(), flowsByFile: new Map(), journeyOfFlow: new Map(), comps: new Map(),
    selected: null, ask: new Set(), askReasons: new Map(),
    selFlow: null, selComp: null,
    tree: { dims: 3, open: new Set(['root']), links: false, highlight: null, inited: null },
    hist: null, histOpen: new Set(), histFilter: 'all', histQuery: '', histPart: '',
    recent: new Map(), activity: [], projects: null,
  };
  const files = () => [...S.byId.values()].filter((n) => n.type === 'file');
  const compInfo = (id) => S.comps.get(id) || { id, name: cap(String(id || 'other').replace(/_/g, ' ')), plain: '' };

  // ---------- loading + live updates ----------
  function setGraph(g) {
    S.meta = g.meta; S.project = g.meta.project; S.version = g.meta.version;
    S.byId = new Map(g.nodes.map((n) => [n.id, n]));
    S.links = g.links;
    S.arch = g.arch || S.arch; S.flows = g.flows || []; S.journeys = g.journeys || []; S.checks = g.checks || [];
    S.comps = new Map((S.arch.components || []).map((c) => [c.id, c]));
    for (const p of S.arch.pipeline || []) if (!S.comps.has(p.id)) S.comps.set(p.id, { id: p.id, name: p.name, plain: p.plain, count: 0, virtual: p.virtual });
    S.flowById = new Map(S.flows.map((f) => [f.id, f]));
    S.flowsByFile = new Map();
    for (const f of S.flows) for (const p of f.files || []) { if (!S.flowsByFile.has(p)) S.flowsByFile.set(p, []); S.flowsByFile.get(p).push(f.id); }
    S.journeyOfFlow = new Map();
    for (const j of S.journeys) for (const id of j.flows) S.journeyOfFlow.set(id, j);
    S.importedBy = new Map();
    for (const l of S.links) if (l.type === 'import') { const t = idOf(l.target); if (!S.importedBy.has(t)) S.importedBy.set(t, []); S.importedBy.get(t).push(idOf(l.source)); }
    for (const id of [...S.ask]) if (!S.byId.has(id)) S.ask.delete(id);
    if (S.selected && !S.byId.has(S.selected) && !S.selected.startsWith('c:')) closeInspector();
    if (S.selFlow && !S.flowById.has(S.selFlow)) S.selFlow = null;
    S.tree.dirty = true;
  }

  async function loadGraph() {
    const g = await api('/api/graph');
    setGraph(g);
    renderChrome();
    renderView();
  }

  function logEvents(events) {
    for (const e of events) {
      if (e.reason === 'note') continue;
      const group = (ids, k, verb) => {
        if (!ids.length) return;
        if (ids.length <= 2) { for (const id of ids) S.activity.unshift({ k, text: `${verb} ${id}`, at: e.at, ids: [id] }); return; }
        const parts = ids.map((i) => i.split('/').slice(0, -1));
        let c = 0; while (c < parts[0].length && parts.every((p) => p[c] === parts[0][c])) c++;
        const where = parts[0].slice(0, c).join('/');
        S.activity.unshift({ k, text: `${verb} ${plural(ids.length, 'file')}${where ? ` in ${where}/` : ''}`, at: e.at, ids });
      };
      if (e.reason === 'hidden') { group(e.removed, 'hidden', 'Hid'); group(e.added, 'shown', 'Showing again:'); }
      else {
        group(e.added, 'added', 'Added'); group(e.modified, 'modified', 'Changed'); group(e.removed, 'removed', 'Deleted');
        if (e.reason === 'watch') { const now = Date.now(); for (const id of [...e.added, ...e.modified]) S.recent.set(id, now); }
      }
    }
    S.activity = S.activity.slice(0, 14);
  }

  let polling = false;
  async function poll() {
    if (polling) return;
    polling = true;
    const live = $('#live');
    try {
      const r = await api(`/api/poll?since=${S.version}`);
      live.textContent = 'Live'; live.classList.add('on');
      if (r.project !== S.project) {
        S.activity = []; S.ask.clear(); S.selFlow = null; S.selComp = null; S.hist = null; S.tree.open = new Set(['root']); S.tree.inited = null; S.tree.highlight = null;
        closeInspector(); await loadGraph(); toast('Project opened');
      } else if (r.version > S.version) {
        logEvents(r.events);
        const g = await api('/api/graph');
        setGraph(g);
        if (r.events.some((e) => e.reason === 'watch')) S.hist = null;
        renderChrome();
        renderView(true);
        if (S.selected && r.events.some((e) => e.modified.includes(S.selected))) openFile(S.selected, false);
      }
    } catch (e) {
      if (/No project/.test(e.message)) { live.textContent = 'No project'; live.classList.remove('on'); }
      else { live.textContent = 'Reconnecting'; live.classList.remove('on'); }
    } finally { polling = false; }
  }

  // ---------- chrome ----------
  const VIEW_TITLES = {
    overview: ['Overview', 'What this project is and how people move through it'],
    architecture: ['Architecture', 'The parts of the design and the path data takes'],
    flows: ['Flows', 'Every journey, traced end to end'],
    tree: ['Tree', 'Project, parts, groups, files'],
    history: ['History', 'Every version, in plain words'],
    ask: ['Ask AI', 'Hand your AI chat exactly the right files'],
    memory: ['Memory', 'Teach CodeBase what your files do'],
    cleanup: ['Clean up', 'Hide generated and noisy files'],
  };
  function renderChrome() {
    $('#projectName').textContent = S.meta ? S.meta.name : 'No project';
    $('#projectKind').textContent = S.meta && S.meta.archetype ? S.meta.archetype.name : '';
    const c = $('#askCount'); c.hidden = !S.ask.size; c.textContent = S.ask.size;
    const errs = S.checks.filter((x) => x.severity === 'error').length, warns = S.checks.filter((x) => x.severity === 'warn').length;
    const cc = $('#checkCount'); cc.hidden = !(errs + warns); cc.textContent = errs + warns; cc.className = 'badge ' + (errs ? 'bad' : 'warn');
    cc.title = `${plural(errs, 'problem')} and ${plural(warns, 'warning')} in the design`;
    const [t, sub] = VIEW_TITLES[S.view];
    $('#viewTitle').textContent = t; $('#viewSub').textContent = sub;
    document.title = S.meta ? `${S.meta.name} · CodeBase` : 'CodeBase';
  }

  function go(view) {
    S.view = view;
    $$('.nav button').forEach((b) => b.classList.toggle('active', b.dataset.view === view));
    $$('.view').forEach((v) => { v.hidden = v.id !== `view-${view}`; });
    renderChrome();
    renderView();
  }
  $$('.nav button').forEach((b) => b.addEventListener('click', () => go(b.dataset.view)));

  function renderView(live = false) {
    if (!S.meta) { renderNoProject(); return; }
    if (S.view === 'overview') renderOverview();
    else if (S.view === 'architecture') renderArchitecture();
    else if (S.view === 'flows') renderFlows(live);
    else if (S.view === 'tree') renderTree();
    else if (S.view === 'history') renderHistory();
    else if (S.view === 'ask' && !live) renderAsk();
    else if (S.view === 'ask') { if (S.pack) rebuildSoon(); }
    else if (S.view === 'memory' && live && $('#memCoverage')) $('#memCoverage').innerHTML = coverageHtml();
    else if (S.view === 'memory') renderMemory();
    else if (S.view === 'cleanup') renderCleanup();
  }

  function renderNoProject() {
    const v = $(`#view-${S.view}`) || $('#view-overview');
    v.innerHTML = `<div class="page narrow"><div class="callout"><h3>Open a project to begin</h3><p>Choose a folder on this computer, or paste a GitHub link. CodeBase works out how the project is built, traces its flows, reads its history, and keeps everything live as files change.</p><div class="actions" style="margin:0"><button class="btn primary" data-act="pick">Open folder</button><button class="btn" data-act="link">Open a link or path</button></div></div></div>`;
  }

  // theme
  const THEMES = ['system', 'light', 'dark'];
  function applyTheme(t) {
    if (t === 'system') delete document.documentElement.dataset.theme; else document.documentElement.dataset.theme = t;
    try { if (t === 'system') localStorage.removeItem('cb-theme'); else localStorage.setItem('cb-theme', t); } catch { /* private mode */ }
    $('#themeBtn').textContent = `Theme: ${t}`;
    readColors();
    if (S.view === 'tree') { treeRecolor(); renderTree(); }
  }
  $('#themeBtn').addEventListener('click', () => {
    const cur = document.documentElement.dataset.theme || 'system';
    applyTheme(THEMES[(THEMES.indexOf(cur) + 1) % THEMES.length]);
  });
  matchMedia('(prefers-color-scheme: light)').addEventListener('change', () => { readColors(); if (S.view === 'tree') { treeRecolor(); renderTree(); } });

  // ---------- project switcher ----------
  const menu = $('#projectMenu'), projectBtn = $('#projectBtn');
  async function openMenu() {
    try { S.projects = await api('/api/projects'); } catch { S.projects = { recent: [], canPick: false }; }
    $('#appVersion').textContent = S.projects.version ? `Version ${S.projects.version}` : '';
    const recent = (S.projects.recent || []).filter((r) => r !== S.projects.current);
    menu.innerHTML = `
      ${S.projects.canPick ? '<button role="menuitem" data-act="pick">Open folder…</button>' : ''}
      <button role="menuitem" data-act="link">Open a link or path…</button>
      ${recent.length ? `<hr><div class="menu-label">Recent</div>${recent.map((r) => `<button role="menuitem" data-open="${esc(r)}"><span>${esc(r.split(/[\\/]/).filter(Boolean).pop())}</span><span class="path">${esc(r)}</span></button>`).join('')}` : ''}`;
    menu.hidden = false; projectBtn.setAttribute('aria-expanded', 'true');
    menu.querySelector('button')?.focus();
  }
  function closeMenu() { menu.hidden = true; projectBtn.setAttribute('aria-expanded', 'false'); }
  projectBtn.addEventListener('click', () => (menu.hidden ? openMenu() : closeMenu()));
  document.addEventListener('click', (e) => { if (!menu.hidden && !e.target.closest('.switcher')) closeMenu(); });

  async function openTarget(body) {
    toast(body.pick ? 'Choose a folder in the window that opened' : 'Opening…');
    try {
      const r = await api('/api/open', body);
      if (r.cancelled) return;
      await poll();
    } catch (e) { toast(e.message); throw e; }
  }
  document.addEventListener('click', async (e) => {
    const act = e.target.closest('[data-act]');
    if (act && (act.dataset.act === 'pick' || act.dataset.act === 'link')) {
      closeMenu();
      if (act.dataset.act === 'pick') {
        if (S.projects && !S.projects.canPick) { openDialog(); return; }
        openTarget({ pick: true }).catch(() => {});
      } else openDialog();
      return;
    }
    const open = e.target.closest('#projectMenu [data-open]');
    if (open) { closeMenu(); openTarget({ target: open.dataset.open }).catch(() => {}); }
  });

  const dlg = $('#openDialog');
  function openDialog() { $('#openMsg').textContent = ''; $('#openTarget').value = ''; dlg.showModal(); $('#openTarget').focus(); }
  $('#openForm').addEventListener('submit', async (e) => {
    if (e.submitter && e.submitter.value === 'cancel') return;
    e.preventDefault();
    const t = $('#openTarget').value.trim();
    if (!t) { $('#openMsg').textContent = 'Paste a folder path or a link.'; return; }
    $('#openMsg').className = 'msg'; $('#openMsg').textContent = /^(https?:|git@)|github\.com/.test(t) ? 'Downloading with full history. Large projects take a minute…' : 'Opening…';
    $('#openGo').disabled = true;
    try { await openTarget({ target: t }); dlg.close(); }
    catch (err) { $('#openMsg').className = 'msg error'; $('#openMsg').textContent = err.message; }
    finally { $('#openGo').disabled = false; }
  });

  $('#refreshBtn').addEventListener('click', async () => { await api('/api/rescan', {}); S.hist = null; await poll(); toast('Scanned again'); });

  // ---------- search: files, flows, journeys, parts ----------
  const search = $('#search'), results = $('#searchResults');
  let hits = [], cursor = 0;
  function runSearch() {
    const q = search.value.trim().toLowerCase();
    if (!q || !S.meta) { results.hidden = true; return; }
    const js = S.journeys.filter((j) => j.name.toLowerCase().includes(q)).slice(0, 3).map((j) => ({ kind: 'journey', id: j.id, name: j.name, sub: `Journey, ${plural(j.flows.length, 'flow')}`, color: 'var(--accent)' }));
    const cs = (S.arch.components || []).filter((c) => c.name.toLowerCase().includes(q)).slice(0, 3).map((c) => ({ kind: 'part', id: c.id, name: c.name, sub: `Part, ${plural(c.count, 'file')}`, color: famVar(c.id) }));
    const fs = S.flows.filter((f) => f.title.toLowerCase().includes(q)).slice(0, 5).map((f) => ({ kind: 'flow', id: f.id, name: f.title, sub: `${KIND_LABEL[f.kind] || 'Flow'}${S.journeyOfFlow.get(f.id) ? ' in ' + S.journeyOfFlow.get(f.id).name : ''}`, color: 'var(--accent)' }));
    const nodes = [...S.byId.values()].filter((n) => n.type !== 'dir' && (n.id.toLowerCase().includes(q) || (n.note || '').toLowerCase().includes(q)))
      .sort((a, b) => (a.name.toLowerCase().startsWith(q) ? 0 : 1) - (b.name.toLowerCase().startsWith(q) ? 0 : 1) || a.id.length - b.id.length)
      .slice(0, 8).map((n) => ({ kind: 'file', id: n.id, name: n.name, sub: `${compInfo(n.comp).name} · ${n.id}`, color: famVar(n.comp) }));
    hits = [...js, ...cs, ...fs, ...nodes];
    cursor = 0;
    results.innerHTML = hits.length
      ? hits.map((h, i) => `<li role="option" data-i="${i}" aria-selected="${i === 0}"><span class="dot" style="background:${h.color}"></span><span>${esc(h.name)}</span><small>${esc(h.sub)}</small></li>`).join('')
      : '<li><span></span><span class="muted">Nothing matches</span></li>';
    results.hidden = false;
  }
  function pick(i) {
    const h = hits[i]; if (!h) return;
    results.hidden = true; search.value = '';
    if (h.kind === 'journey') { const j = S.journeys.find((x) => x.id === h.id); if (j) { S.selFlow = j.flows[0]; go('flows'); } }
    else if (h.kind === 'part') { S.selComp = h.id; go('architecture'); }
    else if (h.kind === 'flow') { S.selFlow = h.id; go('flows'); }
    else openFile(h.id, true);
  }
  search.addEventListener('input', runSearch);
  search.addEventListener('keydown', (e) => {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      cursor = (cursor + (e.key === 'ArrowDown' ? 1 : -1) + hits.length) % Math.max(hits.length, 1);
      $$('#searchResults li').forEach((li, i) => li.setAttribute('aria-selected', String(i === cursor)));
    } else if (e.key === 'Enter') pick(cursor);
    else if (e.key === 'Escape') { results.hidden = true; search.blur(); }
  });
  results.addEventListener('mousedown', (e) => { const li = e.target.closest('[data-i]'); if (li) pick(Number(li.dataset.i)); });
  search.addEventListener('blur', () => setTimeout(() => { results.hidden = true; }, 150));
  addEventListener('keydown', (e) => {
    const typing = /INPUT|TEXTAREA|SELECT/.test(document.activeElement.tagName);
    if (e.key === '/' && !typing) { e.preventDefault(); search.focus(); }
    if (e.key === 'Escape' && !typing && !dlg.open && !$('#diffDialog').open) { closeMenu(); closeInspector(); }
  });

  // ---------- inspector ----------
  const shell = $('#shell');
  function showInspector(kind) {
    $('#inspector').hidden = false; shell.classList.add('with-inspector');
    $('#inspKind').textContent = kind;
    if (S.view === 'tree') requestAnimationFrame(treeResize);
  }
  function closeInspector() {
    S.selected = null;
    $('#inspector').hidden = true; shell.classList.remove('with-inspector');
    if (S.view === 'tree') { requestAnimationFrame(treeResize); treeRepaint(); }
  }
  $('#inspClose').addEventListener('click', closeInspector);

  function noteEditor(id, n, placeholder) {
    return `<details class="note-edit"><summary>${n.note ? 'Edit the description' : 'Write what this does'}</summary>
      <textarea rows="3" placeholder="${esc(placeholder)}">${esc(n.noteSource === 'you' ? n.note : '')}</textarea>
      <div class="actions"><button class="btn primary small" data-save-note="${esc(id)}">Save</button>${n.note ? `<button class="btn ghost small" data-clear-note="${esc(id)}">Remove</button>` : ''}</div></details>`;
  }
  function aboutBlock(n, fallback) {
    if (n.note) return `<div class="about ${n.noteSource === 'ai' ? 'ai' : 'mine'}"><span class="who">${n.noteSource === 'ai' ? 'Described by AI' : 'Your words'}</span><p>${esc(n.note)}</p></div>`;
    return `<div class="about"><span class="who">Automatic summary</span><p>${esc(fallback)}</p></div>`;
  }
  function fileLink(p) {
    const n = S.byId.get(p);
    return `<button class="link path" data-file="${esc(p)}">${esc(p)}</button>${n ? ` <span class="dot" style="background:${famVar(n.comp)}" title="${esc(compInfo(n.comp).name)}"></span>` : ''}`;
  }
  function linkList(paths) { return `<ul class="list-plain">${paths.map((p) => `<li><span>${fileLink(p)}</span></li>`).join('')}</ul>`; }

  /** Everything a change to this file could reach: files that use it (directly or through others), flows, journeys. */
  function blastRadius(id) {
    const seen = new Set([id]); const q = [id];
    while (q.length && seen.size < 2000) { const x = q.shift(); for (const y of S.importedBy.get(x) || []) if (!seen.has(y)) { seen.add(y); q.push(y); } }
    seen.delete(id);
    const flowIds = new Set(S.flowsByFile.get(id) || []);
    const journeys = new Set([...flowIds].map((f) => S.journeyOfFlow.get(f)).filter(Boolean).map((j) => j.name));
    const tests = [...seen].filter((p) => { const n = S.byId.get(p); return n && ['testcases', 'scenarios', 'steps'].includes(n.comp); });
    return { files: seen, flows: [...flowIds], journeys: [...journeys], tests };
  }

  async function openFile(id, fly) {
    const n = S.byId.get(id); if (!n) return;
    if (n.type === 'dir') { openFolder(id); return; }
    S.selected = id;
    if (S.view === 'tree') { treeReveal([id]); if (fly) treeFocus(id); }
    showInspector(n.type === 'asset' ? 'Media file' : 'File');
    const body = $('#inspBody');
    body.innerHTML = '<p class="muted">Opening…</p>';
    let f;
    try { f = await api('/api/file?path=' + encodeURIComponent(id)); } catch (e) { body.innerHTML = `<p class="msg error">${esc(e.message)}</p>`; return; }
    if (S.selected !== id) return;
    const c = compInfo(n.comp);
    const br = blastRadius(id);
    const inAsk = S.ask.has(id);
    const testsAll = [...new Set([...f.testedBy, ...br.tests])];
    body.innerHTML = `
      <div class="crumbs">${esc(f.path.includes('/') ? f.path.slice(0, f.path.lastIndexOf('/') + 1) : './')}</div>
      <h2>${esc(base(f.path))}</h2>
      <div class="chips">
        <button class="chip" data-part="${esc(n.comp)}" title="${esc(c.plain)}"><i style="background:${famVar(n.comp)}"></i>${esc(c.name)}</button>
        <span class="chip">${esc(f.lang)}</span>
        ${f.lines ? `<span class="chip">${plural(f.lines, 'line')}</span>` : ''}
        ${f.generated ? '<span class="chip warn">Looks generated</span>' : ''}
        ${f.importance >= 0.6 ? '<span class="chip key" title="Many other files depend on this one">Key file</span>' : ''}
      </div>
      ${aboutBlock(f, f.summary)}
      ${f.noteStale ? `<div class="stale"><span>This file changed after it was described.</span><button class="btn small" data-confirm-note="${esc(id)}">Still accurate</button></div>` : ''}
      ${c.plain ? `<div class="explain"><b>${esc(c.name)}:</b> ${esc(c.plain)}.</div>` : ''}
      ${f.kind === 'code' ? `<div class="blast"><h4>If you change this file</h4><div class="nums">
        <div><b>${br.files.size}</b><span>files use it</span></div>
        <div><b>${br.flows.length}</b><span>flows pass through</span></div>
        <div><b>${br.journeys.length}</b><span>journeys</span></div>
        <div><b>${testsAll.length}</b><span>tests to re-run</span></div></div>
        ${br.journeys.length ? `<p class="small-note muted">Check ${esc(joinList(br.journeys.slice(0, 5)))}${br.journeys.length > 5 ? ' and more' : ''} after changing it.</p>` : ''}</div>` : ''}
      ${noteEditor(id, f, 'Assigns the nearest free driver to a new ride')}
      <div class="actions">
        <button class="btn primary small" data-copy-context="${esc(id)}" title="Copy this file and the files around it, ready to paste into an AI chat">Copy context for this</button>
        <button class="btn small" data-ask-toggle="${esc(id)}">${inAsk ? 'Unpin from Ask AI' : 'Pin to Ask AI'}</button>
        <button class="btn small" data-tree-show="${esc(id)}">Show on tree</button>
        <a class="btn small" href="vscode://file/${encodeURI(f.abs.replace(/\\/g, '/'))}">Open in VS Code</a>
        <button class="btn ghost small" data-hide="${esc(id)}">Hide</button>
      </div>
      ${br.flows.length ? `<details open><summary>In ${plural(br.flows.length, 'flow')}</summary><ul class="list-plain">${br.flows.slice(0, 30).map((fid) => { const fl = S.flowById.get(fid); const j = S.journeyOfFlow.get(fid); return `<li><button class="link" data-flow="${esc(fid)}">${esc(fl.title)}</button><small>${esc(j ? j.name : KIND_LABEL[fl.kind])}</small></li>`; }).join('')}</ul></details>` : ''}
      <details ${f.imports.length + f.importedBy.length ? 'open' : ''}>
        <summary>Uses ${f.imports.length}, used by ${f.importedBy.length}</summary>
        ${f.imports.length ? `<p class="muted small-note">Uses</p>${linkList(f.imports)}` : ''}
        ${f.importedBy.length ? `<p class="muted small-note">Used by</p>${linkList(f.importedBy)}` : ''}
        ${!f.imports.length && !f.importedBy.length ? '<p class="muted">No links found. Frameworks sometimes connect files in ways a scanner cannot see.</p>' : ''}
      </details>
      ${testsAll.length || f.checks.length ? `<details><summary>${testsAll.length ? `Tests that reach it (${testsAll.length})` : `Checks ${plural(f.checks.length, 'file')}`}</summary>
        ${testsAll.length ? linkList(testsAll.slice(0, 30)) : ''}${f.checks.length ? `<p class="muted small-note">Checks</p>${linkList(f.checks)}` : ''}</details>` : ''}
      ${f.cochange.length ? `<details><summary>Usually changed together with ${plural(f.cochange.length, 'file')}</summary>
        <ul class="list-plain">${f.cochange.map((x) => `<li><span>${fileLink(x.path)}</span><small>${x.commits} commits</small></li>`).join('')}</ul>
        <p class="muted small-note">From Git history. When you change this file, check these too.</p></details>` : ''}
      ${f.lastCommit ? `<p class="muted small-note">Last change: “${esc(f.lastCommit)}”</p>` : ''}
      ${f.symbols.length ? `<details><summary>Defines ${f.symbols.length}</summary><ul class="symbols">${f.symbols.map((s) => `<li>${esc(s.sig)}</li>`).join('')}</ul></details>` : ''}
      ${f.history.length ? `<details><summary>Past prompts (${f.history.length})</summary><ul class="list-plain">${f.history.map((h) => `<li><span>${esc(h.task)}</span><small>${new Date(h.at).toLocaleDateString()}</small></li>`).join('')}</ul></details>` : ''}
      ${f.content != null ? `<div class="code"><pre><code id="codeBlock"></code></pre><div class="more" id="codeMore" hidden></div></div>` : ''}`;
    if (f.content != null) {
      const MAX = 3000;
      const lines = f.content.split('\n');
      const shown = lines.slice(0, MAX).join('\n');
      const block = $('#codeBlock');
      block.textContent = shown;
      if (lines.length > MAX) { $('#codeMore').hidden = false; $('#codeMore').textContent = `Showing the first ${MAX} of ${lines.length} lines.`; }
      const lang = { blade: 'php', vue: 'xml', svelte: 'xml', config: 'ini', shell: 'bash', csharp: 'csharp', gherkin: 'gherkin' }[f.lang] || f.lang;
      if (window.hljs && shown.length < 150000 && hljs.getLanguage(lang)) { try { block.innerHTML = hljs.highlight(shown, { language: lang }).value; } catch { /* plain */ } }
    }
  }

  function openFolder(id) {
    const n = S.byId.get(id); if (!n) return;
    S.selected = id;
    showInspector('Folder');
    const prefix = id === '.' ? '' : id + '/';
    const inside = files().filter((f) => f.id.startsWith(prefix));
    const comps = {};
    for (const f of inside) comps[f.comp] = (comps[f.comp] || 0) + 1;
    const kids = [...S.byId.values()].filter((x) => x.id !== id && x.id.startsWith(prefix) && !x.id.slice(prefix.length).includes('/'));
    $('#inspBody').innerHTML = `
      <div class="crumbs">${esc(id.includes('/') ? id.slice(0, id.lastIndexOf('/') + 1) : './')}</div>
      <h2>${esc(id === '.' ? S.meta.name : n.name + '/')}</h2>
      <div class="chips"><span class="chip">${plural(inside.length, 'file')}</span>${Object.entries(comps).sort((a, b) => b[1] - a[1]).slice(0, 8).map(([c, k]) => `<button class="chip" data-part="${esc(c)}"><i style="background:${famVar(c)}"></i>${k} ${esc(compInfo(c).name.toLowerCase())}</button>`).join('')}</div>
      ${aboutBlock(n, 'No description yet. Memory describes every main folder in one step.')}
      ${noteEditor(id, n, 'Admin screens for approving bills')}
      <div class="actions">
        <button class="btn primary small" data-copy-context="${esc(id)}">Copy context for this</button>
        ${id !== '.' ? `<button class="btn ghost small" data-hide="${esc(id)}/">Hide folder</button>` : ''}
      </div>
      <ul class="list-plain">${kids.sort((a, b) => (a.type === 'dir' ? 0 : 1) - (b.type === 'dir' ? 0 : 1) || a.name.localeCompare(b.name)).map((k) => `<li><span>${k.type === 'dir' ? `<button class="link path" data-file="${esc(k.id)}">${esc(k.name)}/</button>` : fileLink(k.id)}</span><small>${k.type === 'dir' ? '' : esc(compInfo(k.comp).name)}</small></li>`).join('')}</ul>`;
  }

  // Shared click handling for links rendered anywhere.
  document.addEventListener('click', async (e) => {
    const t = e.target.closest('[data-file],[data-flow],[data-part],[data-journey],[data-ask-toggle],[data-copy],[data-hide],[data-save-note],[data-clear-note],[data-confirm-note],[data-go],[data-tree-show]');
    if (!t) return;
    const d = t.dataset;
    if (d.file !== undefined) { const n = S.byId.get(d.file); if (n && n.type === 'dir') openFolder(d.file); else openFile(d.file, true); }
    else if (d.flow !== undefined) { S.selFlow = d.flow; go('flows'); }
    else if (d.part !== undefined) { S.selComp = d.part; go('architecture'); setTimeout(() => $('#compDetail')?.scrollIntoView({ block: 'start', behavior: reduceMotion ? 'auto' : 'smooth' }), 30); }
    else if (d.journey !== undefined) { const j = S.journeys.find((x) => x.id === d.journey); if (j) { S.selFlow = j.flows[0]; go('flows'); } }
    else if (d.askToggle !== undefined) { toggleAsk(d.askToggle); openFile(d.askToggle, false); }
    else if (d.treeShow !== undefined) { showOnTree(base(d.treeShow), [d.treeShow]); }
    else if (d.copy !== undefined) { await copyText(d.copy); toast('Path copied'); }
    else if (d.hide !== undefined) { await addHidden(d.hide); }
    else if (d.saveNote !== undefined || d.clearNote !== undefined) {
      const id = d.saveNote ?? d.clearNote;
      const text = d.clearNote !== undefined ? '' : (t.closest('details')?.querySelector('textarea') || {}).value || '';
      try { await api('/api/note', { path: id, note: text }); await poll(); toast(text ? 'Saved' : 'Removed'); reopen(id); } catch (err) { toast(err.message); }
    }
    else if (d.confirmNote !== undefined) { await api('/api/note', { path: d.confirmNote, confirm: true }); await poll(); reopen(d.confirmNote); }
    else if (d.go !== undefined) go(d.go);
  });
  function reopen(id) { const n = S.byId.get(id); if (!n || S.selected !== id) return; if (n.type === 'dir') openFolder(id); else openFile(id, false); }

  function toggleAsk(id) {
    if (S.ask.has(id)) S.ask.delete(id); else { S.ask.add(id); S.askReasons.set(id, 'You added it'); }
    renderChrome();
    if (S.pack) S.pack = null;
    toast(S.ask.has(id) ? `Pinned ${base(id)} to Ask AI` : `Unpinned ${base(id)}`);
  }

  // Seeds for context packs: the files that matter most, in data-path order.
  const byImportance = (a, b) => ((S.byId.get(b) || {}).importance || 0) - ((S.byId.get(a) || {}).importance || 0);
  function flowSeeds(f) { return (f.files || []).filter((p) => S.byId.has(p)).slice(0, 14); }
  function partSeeds(comp) { return files().filter((n) => n.comp === comp && !n.generated).map((n) => n.id).sort(byImportance).slice(0, 12); }
  function journeyFiles(jid) {
    const j = S.journeys.find((x) => x.id === jid); if (!j) return [];
    return [...new Set(j.flows.flatMap((id) => (S.flowById.get(id) || {}).files || []))];
  }

  // =====================================================================
  // The line: journeys in the order people meet them
  // =====================================================================
  function journeyLine(current) {
    if (!S.journeys.length) return '<p class="muted">No journeys found yet. They appear when CodeBase finds screens, routes, scenarios or tests.</p>';
    return `<div class="line" role="list">${S.journeys.map((j) => {
      const kinds = new Set(j.flows.map((id) => (S.flowById.get(id) || {}).kind));
      const what = kinds.has('scenario') || kinds.has('test') || kinds.has('maestro') ? 'tests' : kinds.has('screen') ? 'screens' : 'requests';
      return `<button class="stop ${current && current.id === j.id ? 'on' : ''} ${j.custom ? 'custom' : ''}" role="listitem" data-journey="${esc(j.id)}" title="${esc(j.about || j.name)}">
        <span class="knob"></span><b>${esc(j.name)}</b><small>${plural(j.flows.length, 'flow')}, ${what}</small></button>`;
    }).join('')}</div>`;
  }

  // =====================================================================
  // Overview
  // =====================================================================
  function renderOverview() {
    const m = S.meta, fl = files(), root = S.byId.get('.') || {};
    const desc = root.note || m.description;
    const source = root.note ? (root.noteSource === 'ai' ? 'Described by AI' : 'Your words') : m.description ? `From ${m.descriptionSource}` : '';
    const pipe = (S.arch.pipeline || []).filter((p) => !p.virtual);
    const parts = (S.arch.components || []).filter((c) => c.count > 0);
    const errs = S.checks.filter((c) => c.severity === 'error').length;
    const warns = S.checks.filter((c) => c.severity === 'warn').length;
    const starts = [];
    const firstOf = (comp) => fl.filter((n) => n.comp === comp).sort(byImportance)[0];
    for (const c of ['entry', 'routes', 'scenarios', 'screens', 'ml_train', 'cli']) { const n = firstOf(c); if (n) starts.push([n, compInfo(c).name]); }
    const key = fl.filter((n) => n.inDeg > 1 && !['testcases', 'scenarios', 'docs', 'config'].includes(n.comp) && !starts.some(([s]) => s === n)).sort(byImportance).slice(0, 6 - Math.min(starts.length, 3));
    const described = fl.filter((n) => n.note).length;
    const pct = fl.length ? Math.round((described / fl.length) * 100) : 0;
    const kindSentence = pipe.length >= 2
      ? `<b>${esc(m.archetype.name)}.</b> Data moves through ${plural(pipe.length, 'part')}: ${esc(pipe.map((p) => p.name).join(' → '))}.`
      : `<b>${esc(m.archetype.name)}.</b>`;

    $('#view-overview').innerHTML = `<div class="page">
      ${m.truncated ? `<div class="callout warn" style="margin-bottom:20px"><h3>Only the first ${m.maxFiles.toLocaleString()} files are mapped</h3><p>Hide folders you do not work on from Clean up, so the design and flows cover the parts that matter.</p><button class="btn small" data-go="cleanup">Open Clean up</button></div>` : ''}
      <div class="hero">
        <h2>${esc(m.name)}</h2>
        ${desc ? `<p class="what">${esc(desc)} ${source ? `<span class="chip">${esc(source)}</span>` : ''}</p>` : `<p class="what muted">No description yet. One line here tells anyone opening this project what it is for.</p>`}
        <p class="kind">${kindSentence}</p>
        <details class="note-edit" style="max-width:640px;border:0;padding:0 0 10px"><summary class="muted" style="font-weight:400">${root.note ? 'Edit the description' : 'Write your own description'}</summary>
          <textarea rows="2" placeholder="Books rides: riders request a trip, the nearest driver is assigned, payment happens at drop-off">${esc(root.noteSource === 'you' ? root.note : '')}</textarea>
          <div class="actions"><button class="btn primary small" data-save-note=".">Save</button></div></details>
        <div class="stack-line">${m.stack.map((s) => `<span class="chip">${esc(s)}</span>`).join('')}</div>
      </div>

      <div class="hero-line">
        <h3><span>How people move through it</span><span class="muted">${S.journeys.some((j) => j.custom) ? 'Named by you or your AI chat' : 'Ordered automatically: sign up, log in, browse, create, assign, pay'}</span></h3>
        ${journeyLine(null)}
      </div>

      <div class="facts">
        <button data-go="tree"><b>${fl.length}</b><span>files</span></button>
        <button data-go="architecture"><b>${parts.length}</b><span>parts in the design</span></button>
        <button data-go="flows"><b>${S.flows.length}</b><span>flows traced end to end</span></button>
        <button data-go="architecture"><b style="color:${errs ? 'var(--bad)' : warns ? 'var(--warn)' : 'var(--ok)'}">${errs + warns}</b><span>${errs ? 'problems in the design' : 'design warnings'}</span></button>
      </div>

      <div class="section two-col">
        <div>
          <div class="section-head"><h2>Needs attention</h2><button class="link muted" data-go="architecture">All checks</button></div>
          ${S.checks.length ? `<ul class="attn">${S.checks.slice(0, 5).map((c) => `<li><span class="dot" style="background:var(--${c.severity === 'error' ? 'bad' : c.severity === 'warn' ? 'warn' : 'info'})"></span><b>${esc(c.title)}</b><span></span><p>${esc(c.fix)}</p></li>`).join('')}</ul>`
            : '<p class="muted">Nothing found. The parts connect the way this kind of project usually does.</p>'}
        </div>
        <div>
          <div class="section-head"><h2>Start reading here</h2><span class="muted">Where things begin</span></div>
          ${starts.length || key.length ? `<ul class="list-plain">${[...starts.slice(0, 4), ...key.map((n) => [n, `used by ${n.inDeg} files`])].map(([n, why]) => `<li><span>${fileLink(n.id)}</span><small>${esc(why)}</small></li>`).join('')}</ul>` : '<p class="muted">No obvious starting files.</p>'}
        </div>
      </div>

      <div class="section two-col">
        <div>
          <div class="section-head"><h2>Recent changes</h2><button class="link muted" data-go="history">Full history</button></div>
          ${S.activity.length ? `<ul class="list-plain">${S.activity.slice(0, 7).map((a) => `<li><span>${esc(a.text)}</span><small>${ago(a.at)}</small></li>`).join('')}</ul>` : '<p class="muted">Nothing since CodeBase opened. Edit a file and it shows here; History has every earlier version.</p>'}
        </div>
        <div>
          <div class="section-head"><h2>Get help from an AI chat</h2></div>
          <p class="muted" style="margin:6px 0 10px">Describe a change or paste an error. CodeBase picks the files that matter, adds the design and the journeys, and fits it to your chat's size.</p>
          <div class="actions" style="margin:0"><button class="btn primary small" data-go="ask">Open Ask AI</button>${pct < 20 && fl.length > 3 ? '<button class="btn small" data-go="memory">Describe the project with AI</button>' : ''}</div>
        </div>
      </div>
    </div>`;
  }

  // =====================================================================
  // Architecture
  // =====================================================================
  function station(id, extra = '') {
    const c = compInfo(id);
    const virt = (S.arch.pipeline || []).find((p) => p.id === id && p.virtual);
    return `<button class="station ${virt ? 'virtual' : ''} ${S.selComp === id ? 'sel' : ''}" style="--c:${famVar(id)}" ${virt ? 'disabled' : `data-sel-comp="${esc(id)}"`}>
      <b>${esc(c.name)}</b><small>${esc(extra || c.plain || '')}</small><span class="n">${virt ? 'Not in this project' : plural(c.count || 0, 'file')}</span></button>`;
  }

  function renderArchitecture() {
    const a = S.arch;
    const pipe = a.pipeline || [];
    const inPipe = new Set(pipe.map((p) => p.id));
    const sideIds = new Set((a.sides || []).map((s) => s.id));
    const others = (a.components || []).filter((c) => !inPipe.has(c.id) && !sideIds.has(c.id) && c.count > 0);
    if (!S.selComp || !S.comps.has(S.selComp)) S.selComp = (pipe.find((p) => !p.virtual) || a.components[0] || {}).id || null;
    // A hop counts as seen when anything earlier on the path reaches this part: an import,
    // or (for the server side) a request a screen sends that a route answers.
    const httpLinks = S.flows.filter((f) => f.kind === 'screen').reduce((n, f) => n + (f.routes || []).length, 0);
    const hops = pipe.slice(1).map((p, i) => {
      const before = new Set(pipe.slice(0, i + 1).map((x) => x.id));
      const c = S.comps.get(p.id) || {};
      let links = Object.entries(c.usedBy || {}).filter(([k]) => before.has(k)).reduce((n, [, v]) => n + v, 0);
      let via = '';
      if (!links && before.has('apiclient') && ['routes', 'controllers', 'middleware'].includes(p.id) && httpLinks) { links = httpLinks; via = 'HTTP'; }
      if (p.virtual) return { seen: (a.path[i] || {}).seen, links: 0, via: '' };
      return { seen: links > 0, links, via };
    });
    const sev = { error: 0, warn: 1, info: 2 };
    const checks = [...S.checks].sort((x, y) => sev[x.severity] - sev[y.severity]);

    $('#view-architecture').innerHTML = `<div class="page">
      <p class="kind" style="max-width:72ch;margin:0 0 6px;color:var(--text-2)">This is a <b style="color:var(--text)">${esc(S.meta.archetype.name.toLowerCase())}</b>. CodeBase sorted every file into the part it plays, from what the code does and what it is called, not which folder it sits in.</p>
      <div class="section" style="margin-top:22px">
        <div class="section-head"><h2>The path data takes</h2><span class="muted">Left to right. A dashed arrow means the link is expected but no import was found.</span></div>
        ${pipe.length ? `<div class="pipe-wrap"><div class="pipe">${pipe.map((p, i) => {
          const hop = i ? hops[i - 1] : null;
          return `${hop ? `<div class="hop ${hop.seen ? '' : 'unseen'}" title="${hop.via ? `${plural(hop.links, 'request')} from screens answered here` : hop.seen && hop.links ? plural(hop.links, 'link') + ' into this part' : hop.seen ? '' : 'No link found into this part'}"><span>${hop.via || (hop.links || '')}</span><i></i></div>` : ''}${station(p.id)}`;
        }).join('')}</div></div>` : '<p class="muted">No main path found. The parts below still show what each file does.</p>'}
        ${(a.sides || []).length ? `<p class="muted" style="margin:18px 0 0;font-size:13px">Supporting parts</p><div class="sides">${a.sides.map((s) => station(s.id, cap(s.how))).join('')}</div>` : ''}
        ${others.length ? `<p class="muted" style="margin:18px 0 0;font-size:13px">Everything else</p><div class="sides">${others.map((c) => station(c.id)).join('')}</div>` : ''}
      </div>
      <div id="compDetail">${compDetail(S.selComp)}</div>
      <div class="section">
        <div class="section-head"><h2>Design checks</h2><span class="muted">${S.checks.length ? plural(S.checks.length, 'finding') : 'Nothing found'}</span></div>
        ${checks.length ? `<ul class="checks">${checks.map((c) => `<li class="check ${c.severity}">
          <h3>${esc(c.title)}</h3>
          <pre>${esc(c.detail)}</pre>
          <p class="fix"><b>What to do:</b> ${esc(c.fix)}</p>
          ${c.files.length ? `<details><summary>${plural(c.files.length, 'file')}</summary>${linkList(c.files.slice(0, 40))}</details>` : ''}
        </li>`).join('')}</ul>` : '<p class="muted">The parts connect the way this kind of project usually does: no loops, no layers skipped, every scenario step has a definition.</p>'}
      </div>
    </div>`;
  }

  function compGroups(comp) {
    const node = ((S.arch.tree || {}).children || []).find((c) => c.comp === comp);
    if (!node) return [];
    if ((node.children || []).every((c) => c.kind === 'file')) return [['', node.children.map((c) => c.id)]];
    return node.children.map((g) => [g.kind === 'group' ? g.name : '', g.kind === 'group' ? g.children.map((c) => c.id) : [g.id]]);
  }

  function compDetail(id) {
    if (!id) return '';
    const c = compInfo(id);
    const groups = compGroups(id);
    const rel = (obj) => Object.entries(obj || {}).sort((x, y) => y[1] - x[1]);
    const uses = rel(c.uses), usedBy = rel(c.usedBy);
    const fam = famOf(id);
    return `<div class="comp-detail">
      <h3><span class="dot" style="background:${famVar(id)}"></span>${esc(c.name)}</h3>
      <p class="muted" style="margin:0">${esc(c.plain)}. <span class="faint">${esc(FAMILY_NAMES[fam])}.</span></p>
      <div class="actions">
        <button class="btn primary small" data-part-context="${esc(id)}">Copy context for this part</button>
        <button class="btn small" data-tree-part="${esc(id)}">Show on tree</button>
        <button class="btn ghost small" data-describe-part="${esc(id)}">Describe these files with AI</button>
      </div>
      <div class="comp-links">
        <div><h4>Uses</h4>${uses.length ? `<ul class="list-plain">${uses.map(([k, n]) => `<li><button class="link" data-sel-comp="${esc(k)}">${esc(compInfo(k).name)}</button><small>${plural(n, 'link')}</small></li>`).join('')}</ul>` : '<p class="muted small-note">Nothing found.</p>'}</div>
        <div><h4>Used by</h4>${usedBy.length ? `<ul class="list-plain">${usedBy.map(([k, n]) => `<li><button class="link" data-sel-comp="${esc(k)}">${esc(compInfo(k).name)}</button><small>${plural(n, 'link')}</small></li>`).join('')}</ul>` : '<p class="muted small-note">Nothing found.</p>'}</div>
      </div>
      <h4 style="margin:0;font-size:13px;color:var(--text-2)">${plural(c.count || 0, 'file')}</h4>
      <ul class="comp-files">${groups.map(([g, list]) => `${g ? `<li class="grp">${esc(g)}</li>` : ''}${list.map((p) => { const n = S.byId.get(p) || {}; return `<li><button class="link path" data-file="${esc(p)}">${esc(base(p))}</button><div class="muted" style="font-size:12px">${esc(n.note || n.summary || '')}</div></li>`; }).join('')}`).join('')}</ul>
    </div>`;
  }

  document.addEventListener('click', (e) => {
    const sc = e.target.closest('[data-sel-comp]');
    if (sc) {
      S.selComp = sc.dataset.selComp;
      if (S.view !== 'architecture') { go('architecture'); return; }
      $$('.station').forEach((s) => s.classList.toggle('sel', s.dataset.selComp === S.selComp));
      $('#compDetail').innerHTML = compDetail(S.selComp);
      $('#compDetail').scrollIntoView({ block: 'nearest', behavior: reduceMotion ? 'auto' : 'smooth' });
      return;
    }
    const tp = e.target.closest('[data-tree-part]');
    if (tp) { const id = tp.dataset.treePart; S.tree.open.add('c:' + id); showOnTree(compInfo(id).name, files().filter((n) => n.comp === id).map((n) => n.id)); return; }
    const dp = e.target.closest('[data-describe-part]');
    if (dp) { S.memScope = 'part:' + dp.dataset.describePart; go('memory'); }
  });

  // =====================================================================
  // Flows
  // =====================================================================
  function flowHeadline(f) {
    const j = S.journeyOfFlow.get(f.id);
    if (f.kind === 'scenario') return `Scenario${f.subtitle ? ` in “${f.subtitle}”` : ''}, ${f.entry} line ${f.line}`;
    if (f.kind === 'maestro') return `Maestro flow, ${f.entry}`;
    if (f.kind === 'test') return `Test in ${f.entry}, line ${f.line}`;
    if (f.kind === 'request') return `${f.subtitle ? `Handled by ${f.subtitle}. ` : ''}Declared in ${f.entry}, line ${f.line}`;
    if (f.kind === 'screen') return `Screen ${f.subtitle}${j ? '' : ''}`;
    return f.entry;
  }

  function stageFamily(st) { return st.virtual ? 'outside' : famOf(st.comp); }

  function renderFlows(live) {
    const v = $('#view-flows');
    if (!S.flows.length) {
      v.innerHTML = `<div class="page narrow"><div class="empty"><h3>No flows found yet</h3><p>A flow starts where something enters the project: a screen, a web address the server answers, a test scenario, or a Maestro flow. CodeBase follows it through every part to where the data ends up. This project has none of those that CodeBase can read yet. The Architecture and Tree views still show every part.</p></div></div>`;
      return;
    }
    if (!S.selFlow || !S.flowById.has(S.selFlow)) S.selFlow = (S.journeys[0] && S.journeys[0].flows[0]) || S.flows[0].id;
    const keepScroll = live ? ($('.flow-list') || {}).scrollTop : 0;
    const f = S.flowById.get(S.selFlow);
    const j = S.journeyOfFlow.get(f.id);
    const stages = f.stages || [];
    const usedBy = f.kind === 'request' ? S.flows.filter((x) => x.kind === 'screen' && (x.routes || []).includes(f.id)) : [];

    v.innerHTML = `<div class="flows-layout">
      <nav class="flow-list" aria-label="Journeys and flows">${S.journeys.map((jj, i) => `<div class="jr">
        <div class="jr-head"><span class="num">${i + 1}</span>${esc(jj.name)}<small>${jj.flows.length}</small></div>
        ${jj.flows.map((id) => { const x = S.flowById.get(id); if (!x) return ''; return `<button class="fl ${id === f.id ? 'active' : ''} ${(x.gaps || []).length ? 'gap' : ''}" data-pick-flow="${esc(id)}" title="${esc(x.title)}"><span class="k">${esc(KIND_SHORT(x))}</span><span>${esc(x.kind === 'request' && x.path ? x.path : x.title)}</span></button>`; }).join('')}
      </div>`).join('')}</nav>
      <div class="flow-main">
        <div class="flow-top">${journeyLine(j)}</div>
        <h2 class="flow-title">${esc(f.title)}</h2>
        <p class="flow-sub">${esc(flowHeadline(f))}</p>
        <div class="actions">
          <button class="btn primary small" id="playFlow">Play the flow</button>
          <button class="btn small" data-flow-context="${esc(f.id)}">Copy context for this flow</button>
          <button class="btn small" data-flow-ask="${esc(f.id)}">Change something here</button>
          <button class="btn small" data-flow-tree="${esc(f.id)}">Show on tree</button>
        </div>
        ${(f.gaps || []).length ? `<div class="gaps"><b>Breaks in this flow</b><ul>${f.gaps.map((g) => `<li>${esc(g)}</li>`).join('')}</ul></div>` : ''}
        <ol class="ride">${stages.map((st) => {
          const c = compInfo(st.comp);
          return `<li class="${st.virtual ? 'virtual' : ''}" style="--c:var(--f-${stageFamily(st)})">
            <span class="knob" aria-hidden="true"></span>
            <div class="st"><h3>${esc(c.name || st.name)}<small>${esc(st.virtual ? (st.comp === 'server' ? 'The server this app calls; its code is not in this project' : 'The app the tests drive; its code is not in this project') : c.plain || '')}</small></h3>
              ${st.files.length ? `<ul class="files">${st.files.map((x) => `<li>${fileLink(x.path)}${x.note ? `<span class="note">${esc(x.note)}</span>` : ''}</li>`).join('')}</ul>` : ''}</div>
          </li>`;
        }).join('')}</ol>
        ${f.steps && f.steps.length ? `<div class="section" style="margin-top:20px"><div class="section-head"><h2>${f.kind === 'maestro' ? 'Commands' : 'Each line and the code that runs it'}</h2><span class="muted">${plural(f.steps.length, 'line')}</span></div>
          <table class="steps-map"><thead><tr><th>Step</th><th>Text</th><th>${f.kind === 'maestro' ? 'Line' : 'Runs'}</th></tr></thead><tbody>
          ${f.steps.map((s) => `<tr class="${f.kind !== 'maestro' && !s.def ? 'missing' : ''}"><td class="kw">${esc(s.kw)}</td><td>${esc(s.text)}</td><td>${f.kind === 'maestro' ? s.line : s.def ? `<button class="link path" data-file="${esc(s.def)}">${esc(base(s.def))}</button>` : 'No step definition'}</td></tr>`).join('')}
          </tbody></table></div>` : ''}
        ${f.calls && f.calls.length ? `<div class="section" style="margin-top:20px"><div class="section-head"><h2>Requests this screen sends</h2><span class="muted">${plural(f.calls.length, 'request')}</span></div>
          <table class="steps-map"><tbody>${f.calls.map((c) => `<tr class="${c.route ? '' : 'missing'}"><td class="kw">${esc(c.method)}</td><td class="path">${esc(c.url)}</td><td>${c.route ? `<button class="link" data-pick-flow="${esc(c.route)}">${esc((S.flowById.get(c.route) || {}).title || 'Open')}</button>` : (S.flows.some((x) => x.kind === 'request') ? 'No route here answers it' : 'Answered outside this project')}</td></tr>`).join('')}</tbody></table></div>` : ''}
        ${usedBy.length ? `<div class="section" style="margin-top:20px"><div class="section-head"><h2>Screens that send this request</h2></div><ul class="list-plain">${usedBy.map((x) => `<li><button class="link" data-pick-flow="${esc(x.id)}">${esc(x.title)}</button><small>${esc(x.entry)}</small></li>`).join('')}</ul></div>` : ''}
        ${journeyAiPanel()}
      </div>
    </div>`;
    if (keepScroll) $('.flow-list').scrollTop = keepScroll;
    else { const a = $('.flow-list .fl.active'); if (a) a.scrollIntoView({ block: 'center' }); }
  }

  function journeyAiPanel() {
    const custom = S.journeys.some((j) => j.custom);
    return `<details class="ai-panel" ${S.jOpen ? 'open' : ''} id="jPanel"><summary>Name and order the journeys with your AI chat</summary>
      <p class="muted" style="max-width:68ch">CodeBase groups flows by the words they share and orders them through a typical app's life. Your AI chat can do better with the names: copy the prompt, paste it into any chat, and paste the answer back. Nothing else leaves this computer.</p>
      <div class="actions"><button class="btn small" id="jPrompt">Copy the prompt</button>${custom ? '<button class="btn ghost small" id="jReset">Go back to automatic journeys</button>' : ''}</div>
      <textarea id="jReply" class="code-area" rows="5" placeholder='{"journeys":[{"name":"Rider logs in","about":"…","flows":["v:client/src/screens/LoginScreen.jsx", "…"]}]}'></textarea>
      <div class="actions"><button class="btn primary small" id="jSave">Save journeys</button><span id="jMsg" class="msg"></span></div></details>`;
  }

  function journeyPrompt() {
    const lines = S.flows.slice(0, 400).map((f) => {
      const parts = (f.stages || []).filter((s) => !s.virtual).map((s) => `${compInfo(s.comp).name}: ${s.files.map((x) => base(x.path)).slice(0, 3).join(', ')}`).join(' | ');
      return `- id: ${f.id}\n  ${KIND_LABEL[f.kind]}: ${f.title}${f.subtitle ? ` (${f.subtitle})` : ''}\n  passes through: ${parts}`;
    });
    return `<role>
You are a senior engineer explaining how people use an app, by grouping its technical flows into user journeys.
</role>

<project name="${S.meta.name}" kind="${S.meta.archetype.name}" built_with="${S.meta.stack.join(', ')}">
${(S.byId.get('.') || {}).note || S.meta.description || ''}
</project>

<flows>
${lines.join('\n')}
</flows>

<task>
Group these flows into 3 to 12 journeys a user or tester would recognise (for example "Sign up", "Request a ride", "Driver is assigned", "Pay for the trip").
Order the journeys the way someone meets them, from first launch to the end, and order the flows inside each journey the same way.
Give each journey a short plain name (max 5 words) and one sentence saying what happens.
Use every flow id exactly once, copied exactly.
</task>

<answer_format>
Return ONLY JSON, nothing before or after it:
{"journeys":[{"name":"…","about":"…","flows":["id", "id"]}]}
</answer_format>
`;
  }

  let playing = null;
  function playFlow() {
    const items = $$('.ride > li');
    if (!items.length) return;
    clearInterval(playing);
    items.forEach((li) => li.classList.remove('lit'));
    if (reduceMotion) { items.forEach((li) => li.classList.add('lit')); return; }
    let i = 0;
    const step = () => {
      if (i >= items.length) { clearInterval(playing); playing = null; return; }
      items[i].classList.add('lit');
      items[i].scrollIntoView({ block: 'nearest', behavior: 'smooth' });
      i++;
    };
    step();
    playing = setInterval(step, 750);
  }

  document.addEventListener('click', async (e) => {
    const pf = e.target.closest('[data-pick-flow]');
    if (pf) { S.selFlow = pf.dataset.pickFlow; if (S.view !== 'flows') go('flows'); else renderFlows(); $('.flow-main')?.scrollTo({ top: 0 }); return; }
    if (e.target.closest('#playFlow')) { playFlow(); return; }
    const fa = e.target.closest('[data-flow-ask]');
    if (fa) {
      const f = S.flowById.get(fa.dataset.flowAsk); if (!f) return;
      S.packSeeds = flowSeeds(f); S.packSeedReason = `the flow "${f.title}"`; S.packModes.clear(); S.pack = null;
      if (!(S.askGoal || '').trim()) S.askGoal = `In "${f.title}": `;
      go('ask');
      const goal = $('#askGoal'); if (goal) { goal.focus(); goal.setSelectionRange(goal.value.length, goal.value.length); }
      return;
    }
    const ft = e.target.closest('[data-flow-tree]');
    if (ft) { const f = S.flowById.get(ft.dataset.flowTree); if (f) showOnTree(f.title, f.files); return; }
    if (e.target.closest('#jPrompt')) { S.jOpen = true; const ok = await copyText(journeyPrompt()); toast(ok ? 'Prompt copied. Paste it into your AI chat.' : 'Could not copy'); return; }
    if (e.target.closest('#jReset')) { await api('/api/journeys', { journeys: null }); await poll(); toast('Back to automatic journeys'); return; }
    if (e.target.closest('#jSave')) {
      const raw = $('#jReply').value; const msg = $('#jMsg');
      const a = raw.indexOf('{'), b = raw.lastIndexOf('}');
      let parsed = null; try { parsed = JSON.parse(raw.slice(a, b + 1)); } catch { /* below */ }
      const list = parsed && (Array.isArray(parsed) ? parsed : parsed.journeys);
      if (!Array.isArray(list) || !list.length) { msg.className = 'msg error'; msg.textContent = 'No journeys found. Paste the JSON your AI chat returned.'; return; }
      const known = list.reduce((n, j) => n + (j.flows || []).filter((id) => S.flowById.has(id)).length, 0);
      if (!known) { msg.className = 'msg error'; msg.textContent = 'None of those flow ids are in this project. Copy a fresh prompt and try again.'; return; }
      try { await api('/api/journeys', { journeys: list }); S.jOpen = false; await poll(); toast(`Saved ${plural(list.length, 'journey')}`); }
      catch (err) { msg.className = 'msg error'; msg.textContent = err.message; }
    }
  });
  document.addEventListener('toggle', (e) => { if (e.target.id === 'jPanel') S.jOpen = e.target.open; }, true);

  // =====================================================================
  // Tree: project -> parts -> groups -> files. A real m-ary tree with fixed
  // positions, so nothing drifts: 3D puts each node's children on a ring
  // below it (a cone tree); 2D reads left to right.
  // =====================================================================
  const T = { nodes: new Map(), parent: new Map(), graph: null, visible: [], pos: new Map(), hover: null };

  function treeIndex() {
    T.nodes = new Map(); T.parent = new Map();
    const root = S.arch.tree || { id: 'root', kind: 'root', name: S.meta.name, children: [] };
    const walk = (n, parent, comp) => {
      const id = n.id;
      const c = n.comp || comp;
      const kids = (n.children || []).map((ch) => ch.id);
      const name = n.kind === 'file' ? base(id) : n.kind === 'root' ? (n.name || S.meta.name) : n.name;
      T.nodes.set(id, { id, kind: n.kind, name, comp: c, kids, count: n.count || 0 });
      if (parent) T.parent.set(id, parent);
      for (const ch of n.children || []) walk(ch, id, c);
    };
    walk(root, null, null);
    if (S.tree.inited !== S.project) {
      S.tree.inited = S.project;
      S.tree.open = new Set(['root']);
      // Small projects open fully; medium ones open each part one level; big ones show parts only.
      const total = files().length;
      for (const [id, n] of T.nodes) {
        if (n.kind === 'component' && total <= 150) S.tree.open.add(id);
        if (n.kind === 'group' && total <= 60) S.tree.open.add(id);
      }
    }
    S.tree.dirty = false;
  }

  function treeVisible() {
    const out = [];
    const walk = (id, depth) => {
      const n = T.nodes.get(id); if (!n) return;
      n.depth = depth; n.open = S.tree.open.has(id) || id === 'root';
      out.push(n);
      if (n.open) for (const k of n.kids) walk(k, depth + 1);
    };
    walk('root', 0);
    return out;
  }

  function treeReveal(ids) {
    for (const id of ids) { let p = T.parent.get(id); while (p) { S.tree.open.add(p); p = T.parent.get(p); } }
  }

  function descendantFiles(id) {
    const n = T.nodes.get(id); if (!n) return [];
    if (n.kind === 'file') return [id];
    return n.kids.flatMap(descendantFiles);
  }

  function hot(n) {
    const h = S.tree.highlight; if (!h) return true;
    if (n.kind === 'file') return h.ids.has(n.id);
    if (n.kind === 'root') return true;
    return descendantFiles(n.id).some((f) => h.ids.has(f));
  }

  function nodeHex(n) {
    if (n.kind === 'root') return famHex.text;
    if (n.id === S.selected) return famHex.accent;
    if (!hot(n)) return famHex.line;
    return famHex[famOf(n.comp)] || famHex.text3;
  }

  // ---- 3D cone layout ----
  let LEVEL = 120;
  function coneLayout(vis) {
    const byId = new Map(vis.map((n) => [n.id, n]));
    const radius = (n) => {
      const kids = n.open ? n.kids.map((k) => byId.get(k)).filter(Boolean) : [];
      if (!kids.length) { n.r = n.kind === 'file' ? 11 : n.kind === 'group' ? 18 : 26; n.R = 0; return n.r; }
      const rs = kids.map(radius);
      const gap = 16;
      const sum = rs.reduce((a, r) => a + 2 * r + gap, 0);
      const maxr = Math.max(...rs);
      n.R = kids.length === 1 ? 0 : Math.max(sum / (2 * Math.PI), maxr + gap);
      n.r = n.R + maxr;
      return n.r;
    };
    const root = byId.get('root');
    radius(root);
    // Level height grows with the row of parts, so the tree keeps a readable shape at any size.
    const top = root.open ? root.kids.map((k) => byId.get(k)).filter(Boolean) : [];
    const rowWidth = top.reduce((a, k) => a + 2 * Math.max(k.r, 60) + 56, 0);
    LEVEL = Math.max(120, Math.min(520, rowWidth * 0.2));
    const place = (n, x, z, depth) => {
      n.fx = x; n.fy = -depth * LEVEL; n.fz = z;
      const kids = n.open ? n.kids.map((k) => byId.get(k)).filter(Boolean) : [];
      if (!kids.length) return;
      if (n.kind === 'root') {
        // Parts stand in a row, left to right in the order data moves through them,
        // so the tree reads like the Architecture view. Their own pieces ring below them.
        const gapX = 56;
        const width = kids.reduce((a, k) => a + 2 * Math.max(k.r, 60) + gapX, -gapX);
        let cx = x - width / 2;
        for (const k of kids) {
          const w = 2 * Math.max(k.r, 60);
          place(k, cx + w / 2, z, depth + 1);
          cx += w + gapX;
        }
        return;
      }
      const gap = 16;
      const total = kids.reduce((a, k) => a + 2 * k.r + gap, 0);
      let a = -Math.PI / 2;
      for (const k of kids) {
        const span = ((2 * k.r + gap) / total) * Math.PI * 2;
        const ang = a + span / 2;
        place(k, x + n.R * Math.cos(ang), z + n.R * Math.sin(ang), depth + 1);
        a += span;
      }
    };
    place(root, 0, 0, 0);
  }

  function crossLinks(vis) {
    if (!S.tree.links) return [];
    const shown = new Set(vis.map((n) => n.id));
    const rep = (id) => { let x = id; while (x && !shown.has(x)) x = T.parent.get(x); return x; };
    const agg = new Map();
    for (const l of S.links) {
      if (l.type !== 'import') continue;
      const a = rep(idOf(l.source)), b = rep(idOf(l.target));
      if (!a || !b || a === b || a === 'root' || b === 'root') continue;
      const k = a + '\u0000' + b;
      agg.set(k, (agg.get(k) || 0) + 1);
    }
    return [...agg].map(([k, n]) => { const [a, b] = k.split('\u0000'); return { source: a, target: b, type: 'x', n }; });
  }

  function initTree3d() {
    if (T.graph || typeof ForceGraph3D !== 'function') return;
    const el = $('#tree3d');
    T.graph = ForceGraph3D({ controlType: 'orbit' })(el)
      .backgroundColor(famHex.bg)
      .showNavInfo(false)
      .enableNodeDrag(false)
      .nodeId('id')
      .nodeRelSize(5)
      .nodeOpacity(0.95)
      .nodeResolution(16)
      .nodeVal((n) => (n.kind === 'root' ? 22 : n.kind === 'component' ? 7 + Math.min(10, Math.log2((n.count || 1) + 1) * 2) : n.kind === 'group' ? 3.5 : 1))
      .nodeColor(nodeHex)
      .nodeLabel((n) => {
        if (n.kind === 'file') { const f = S.byId.get(n.id) || {}; return `<div class="tip"><b>${esc(n.id)}</b><span>${esc(f.note || f.summary || '')}</span></div>`; }
        const c = compInfo(n.comp);
        return `<div class="tip"><b>${esc(n.kind === 'root' ? S.meta.name : n.kind === 'group' ? `${c.name}: ${n.name}` : c.name)}</b><span>${esc(n.kind === 'root' ? S.meta.archetype.name : `${plural(n.count || descendantFiles(n.id).length, 'file')}. ${n.open ? 'Click to close.' : 'Click to open.'}`)}</span></div>`;
      })
      .linkColor((l) => (l.type === 'x' || l.type === 'path' ? famHex.accent : famHex.line))
      .linkWidth((l) => (l.type === 'path' ? 1.6 : l.type === 'x' ? Math.min(2.5, 0.6 + Math.log2(l.n + 1) * 0.5) : 0.8))
      .linkOpacity(0.75)
      .linkDirectionalParticles((l) => ((l.type === 'x' || l.type === 'path') && !reduceMotion ? 2 : 0))
      .linkDirectionalParticleWidth(1.6)
      .linkDirectionalParticleSpeed(0.006)
      .linkDirectionalParticleColor(() => famHex.accent)
      .cooldownTicks(1)
      .onNodeClick((n) => treeClick(n.id))
      .onNodeHover((n) => { el.style.cursor = n ? 'pointer' : ''; T.hover = n ? n.id : null; })
      .onBackgroundClick(() => { T.hover = null; });
    T.graph.d3Force('charge', null);
    T.graph.d3Force('center', null);
    T.graph.d3Force('link').strength(0);
    requestAnimationFrame(labelLoop);
    addEventListener('resize', treeResize);
  }

  function treeClick(id) {
    const n = T.nodes.get(id); if (!n) return;
    if (n.kind === 'file') { openFile(id, false); return; }
    if (n.kind === 'root') return;
    if (S.tree.open.has(id)) S.tree.open.delete(id); else S.tree.open.add(id);
    renderTree(false);
  }

  let lastFit = null;
  function renderTree(refit = true) {
    if (S.tree.dirty || !T.nodes.size) treeIndex();
    const vis = treeVisible();
    T.visible = vis;
    press('data-tdim', S.tree.dims);
    $('#treeLinks').checked = S.tree.links;
    const fams = new Set(vis.filter((n) => n.kind !== 'root').map((n) => famOf(n.comp)));
    $('#treeLegend').innerHTML = `${Object.keys(FAMILY_NAMES).filter((f) => fams.has(f)).map((f) => `<span><i class="dot" style="background:var(--f-${f})"></i>${esc(FAMILY_NAMES[f])}</span>`).join('')}
      ${S.tree.dims === 3 ? '<span><i class="ln" style="background:var(--accent)"></i>Path data takes</span>' : ''}
      <span class="note">Parts are in the order data moves through them, ${S.tree.dims === 3 ? 'left to right' : 'top to bottom'}. Click a part to open or close it. Click a file for details. ${S.tree.dims === 3 ? 'Drag to turn, scroll to zoom.' : ''}${S.tree.links ? ' Blue lines: one part uses another.' : ''}</span>`;
    const hb = $('#treeBar');
    hb.hidden = !S.tree.highlight;
    if (S.tree.highlight) hb.innerHTML = `<span>Showing <b>${esc(S.tree.highlight.label)}</b> (${plural(S.tree.highlight.ids.size, 'file')})</span><button class="btn ghost small" id="clearTreeHi">Show everything</button>`;

    if (S.tree.dims === 2) {
      $('#tree3d').hidden = true; $('#labels').hidden = true; $('#tree2d').hidden = false;
      render2d(vis);
      return;
    }
    $('#tree2d').hidden = true; $('#tree3d').hidden = false; $('#labels').hidden = false;
    initTree3d();
    if (!T.graph) return;
    coneLayout(vis);
    const nodes = vis.map((n) => n);
    const links = vis.filter((n) => n.id !== 'root').map((n) => ({ source: T.parent.get(n.id), target: n.id, type: 'tree' }));
    // The data path: consecutive parts on the pipeline, joined left to right, with data moving along.
    const shownIds = new Set(vis.map((n) => n.id));
    const pipe = (S.arch.pipeline || []).filter((c) => !c.virtual).map((c) => `c:${c.id}`).filter((id) => shownIds.has(id));
    const path = pipe.slice(1).map((id, i) => ({ source: pipe[i], target: id, type: 'path' }));
    T.graph.graphData({ nodes, links: [...links, ...path, ...crossLinks(vis)] });
    treeResize();
    const key = S.project + ':' + vis.length;
    if (refit || lastFit !== S.project) { lastFit = S.project; setTimeout(() => treeFit(), 60); }
    else if (key !== T.lastKey) { /* keep the camera where the user left it */ }
    T.lastKey = key;
  }

  function treeResize() {
    if (!T.graph || S.view !== 'tree' || S.tree.dims !== 3) return;
    const box = $('#view-tree');
    T.graph.width(box.clientWidth).height(box.clientHeight);
  }
  function treeRecolor() { if (T.graph) { T.graph.backgroundColor(famHex.bg); treeRepaint(); } }
  function treeRepaint() {
    if (S.tree.dims === 2) { if (S.view === 'tree' && T.visible.length) render2d(T.visible); return; }
    if (T.graph) { T.graph.nodeColor(T.graph.nodeColor()); T.graph.linkColor(T.graph.linkColor()); }
  }
  function treeFit() {
    if (S.tree.dims === 2) { $('#tree2d').scrollTo({ top: 0, left: 0 }); return; }
    if (!T.graph) return;
    // Frame the row of parts from slightly above, so each ring of files opens up.
    const h = S.tree.highlight;
    let vis = T.visible.filter((n) => n.fx !== undefined);
    if (h) { const hv = vis.filter((n) => n.kind === 'file' && h.ids.has(n.id)); if (hv.length) vis = hv; }
    if (!vis.length) return;
    const xs = vis.map((n) => n.fx), ys = vis.map((n) => n.fy), zs = vis.map((n) => n.fz);
    const c = { x: (Math.min(...xs) + Math.max(...xs)) / 2, y: (Math.min(...ys) + Math.max(...ys)) / 2, z: (Math.min(...zs) + Math.max(...zs)) / 2 };
    // Fit width and height separately (a wide row of parts is not a sphere), leaving room for labels.
    const box = $('#view-tree');
    const aspect = box.clientWidth / Math.max(1, box.clientHeight);
    const vfov = (T.graph.camera().fov || 40) * Math.PI / 180;
    const hfov = 2 * Math.atan(Math.tan(vfov / 2) * aspect);
    const halfW = Math.max(80, (Math.max(...xs) - Math.min(...xs)) / 2) * 1.12 + 40;
    const halfH = Math.max(60, (Math.max(...ys) - Math.min(...ys)) / 2) * 1.25 + 40;
    const depth = (Math.max(...zs) - Math.min(...zs)) / 2;
    const dist = Math.max(halfW / Math.tan(hfov / 2), halfH / Math.tan(vfov / 2)) + depth;
    // Look down on the cone from above and in front, so every ring opens up.
    const dir = { x: 0, y: 0.38, z: 1 }; const len = Math.hypot(dir.x, dir.y, dir.z);
    T.graph.cameraPosition({ x: c.x + dir.x / len * dist, y: c.y + dir.y / len * dist, z: c.z + dir.z / len * dist }, c, reduceMotion ? 0 : 600);
  }
  function treeFocus(id) {
    if (S.tree.dims === 2) { const el = $(`#tree2d [data-tnode="${CSS.escape(id)}"]`); if (el) el.scrollIntoView({ block: 'center', inline: 'center', behavior: reduceMotion ? 'auto' : 'smooth' }); return; }
    const n = T.visible.find((x) => x.id === id);
    if (!T.graph || !n || n.fx === undefined) return;
    T.graph.cameraPosition({ x: n.fx + 70, y: n.fy + 50, z: n.fz + 160 }, { x: n.fx, y: n.fy, z: n.fz }, reduceMotion ? 0 : 800);
  }

  function showOnTree(label, ids) {
    const set = new Set(ids.filter((id) => S.byId.has(id)));
    if (!set.size) { toast('Nothing to show on the tree'); return; }
    if (S.tree.dirty || !T.nodes.size) treeIndex();
    S.tree.highlight = { label, ids: set };
    treeReveal([...set]);
    go('tree');
  }

  // ---- labels for 3D ----
  const labelLayer = $('#labels');
  const labelEls = new Map();
  function labelLoop() {
    requestAnimationFrame(labelLoop);
    if (!T.graph || S.view !== 'tree' || S.tree.dims !== 3) return;
    const nodes = T.visible;
    if (!nodes.length) return;
    const el = $('#tree3d');
    const w = el.clientWidth, h = el.clientHeight;
    const cam = T.graph.camera();
    const fwd = new cam.position.constructor();
    cam.getWorldDirection(fwd);
    const fileCount = nodes.filter((n) => n.kind === 'file').length;
    const cands = [];
    for (const n of nodes) {
      if (n.fx === undefined) continue;
      let prio;
      if (n.id === S.selected || n.id === T.hover) prio = 0;
      else if (n.kind === 'root') prio = 1;
      else if (n.kind === 'component') prio = 2;
      else if (S.tree.highlight && n.kind === 'file' && S.tree.highlight.ids.has(n.id)) prio = 3;
      else if (n.kind === 'group') prio = 4;
      else if (fileCount <= 36) prio = 5;
      else continue;
      const dx = n.fx - cam.position.x, dy = n.fy - cam.position.y, dz = n.fz - cam.position.z;
      if (dx * fwd.x + dy * fwd.y + dz * fwd.z <= 0) continue;
      cands.push({ n, prio, d: Math.hypot(dx, dy, dz) });
    }
    cands.sort((a, b) => a.prio - b.prio || a.d - b.d);
    const placed = [], used = new Set();
    for (const c of cands) {
      if (used.size >= 80) break;
      const p = T.graph.graph2ScreenCoords(c.n.fx, c.n.fy, c.n.fz);
      if (p.x < -60 || p.y < -20 || p.x > w + 60 || p.y > h + 20) continue;
      const comp = compInfo(c.n.comp);
      const text = c.n.kind === 'root' ? S.meta.name : c.n.kind === 'component' ? `${comp.name}${c.n.open ? '' : `  ${c.n.count}`}` : c.n.kind === 'group' ? `${c.n.name}${c.n.open ? '' : `  ${c.n.kids.length}`}` : c.n.name;
      const big = c.prio <= 2;
      const bw = text.length * (big ? 7.6 : 6.6) + 16, bh = big ? 22 : 18;
      const hit = (b) => placed.some((q) => b.x < q.x + q.w && q.x < b.x + b.w && b.y < q.y + q.h && q.y < b.y + b.h);
      let box = null;
      // Parts stand in a row, so their names go above or below the dot, never sideways into the next part.
      const spots = c.n.kind === 'component' || c.n.kind === 'root'
        ? [[-bw / 2, -bh - 12], [-bw / 2, 12], [12, -bh / 2]]
        : [[12, -bh / 2], [-bw / 2, 10], [-bw / 2, -bh - 10], [-bw - 12, -bh / 2]];
      for (const [ox, oy] of spots) {
        const b = { x: p.x + ox, y: p.y + oy, w: bw, h: bh };
        if (c.prio <= 1 || !hit(b)) { box = b; break; }
      }
      // A part's name always shows, even if it has to overlap a file name.
      if (!box && c.prio <= 2) box = { x: p.x + spots[0][0], y: p.y + spots[0][1], w: bw, h: bh };
      if (!box) continue;
      placed.push(box);
      let lab = labelEls.get(c.n.id);
      if (!lab) { lab = document.createElement('div'); labelLayer.appendChild(lab); labelEls.set(c.n.id, lab); }
      const cls = 'lab ' + (c.prio === 0 ? 'on ' : '') + (c.n.kind === 'root' ? 'root' : c.n.kind === 'component' ? 'comp' : c.n.kind === 'file' ? 'file' : '');
      if (lab.className !== cls) lab.className = cls;
      if (lab.textContent !== text) lab.textContent = text;
      lab.style.setProperty('--c', famVar(c.n.comp));
      lab.style.opacity = hot(c.n) ? '1' : '0.45';
      lab.style.transform = `translate(${Math.round(box.x)}px, ${Math.round(box.y)}px)`;
      used.add(c.n.id);
    }
    for (const [id, lab] of labelEls) if (!used.has(id)) { lab.remove(); labelEls.delete(id); }
  }

  // ---- 2D: left-to-right tree in SVG ----
  function render2d(vis) {
    const byId = new Map(vis.map((n) => [n.id, n]));
    const COL = 270, ROW = 26, PAD = 30;
    let leaf = 0, maxDepth = 0;
    const lay = (n) => {
      maxDepth = Math.max(maxDepth, n.depth);
      n.x = PAD + n.depth * COL;
      const kids = n.open ? n.kids.map((k) => byId.get(k)).filter(Boolean) : [];
      if (!kids.length) { n.y = PAD + leaf++ * ROW; return; }
      kids.forEach(lay);
      n.y = (kids[0].y + kids[kids.length - 1].y) / 2;
    };
    lay(byId.get('root'));
    const W = PAD * 2 + maxDepth * COL + 320, H = PAD * 2 + leaf * ROW;
    const r = (n) => (n.kind === 'root' ? 9 : n.kind === 'component' ? 8 : n.kind === 'group' ? 6 : 4.5);
    const seenNames = new Map();
    for (const n of vis) if (n.kind === 'file') seenNames.set(n.name, (seenNames.get(n.name) || 0) + 1);
    const labelOf = (n) => {
      if (n.kind === 'root') return S.meta.name;
      if (n.kind === 'component') return compInfo(n.comp).name;
      if (n.kind === 'file' && seenNames.get(n.name) > 1) { const dir = n.id.split('/').slice(-2, -1)[0]; return dir ? `${dir}/${n.name}` : n.name; }
      return n.name;
    };
    const textW = (n) => labelOf(n).length * (n.kind === 'file' ? 7.3 : 7.8) + 14;
    const edges = vis.filter((n) => n.id !== 'root').map((n) => {
      const p = byId.get(T.parent.get(n.id)); if (!p) return '';
      const x1 = Math.min(p.x + r(p) + textW(p), n.x - 40), x2 = n.x - r(n), mx = (x1 + x2) / 2;
      return `<path class="edge ${S.tree.highlight && hot(n) && n.kind === 'file' ? 'hot' : ''}" d="M${x1},${p.y} C${mx},${p.y} ${mx},${n.y} ${x2},${n.y}"/>`;
    }).join('');
    const xl = crossLinks(vis).map((l) => {
      const a = byId.get(l.source), b = byId.get(l.target); if (!a || !b) return '';
      const bend = 60 + Math.min(200, Math.abs(a.y - b.y) * 0.25);
      return `<path class="xlink" d="M${a.x},${a.y} C${a.x + bend},${a.y} ${b.x + bend},${b.y} ${b.x},${b.y}"><title>${esc(T.nodes.get(l.source).name)} uses ${esc(T.nodes.get(l.target).name)} (${plural(l.n, 'link')})</title></path>`;
    }).join('');
    const nodes = vis.map((n) => {
      const col = famVar(n.comp);
      const isFile = n.kind === 'file';
      const fill = isFile ? col : 'var(--bg)';
      const stroke = n.kind === 'root' ? 'var(--text)' : col;
      const comp = compInfo(n.comp);
      const label = labelOf(n);
      const sub = n.kind === 'file' ? '' : n.kind === 'root' ? S.meta.archetype.name : n.open ? plural(n.kind === 'component' ? n.count : n.kids.length, 'file') : `${plural(n.kind === 'component' ? n.count : n.kids.length, 'file')}, click to open`;
      const weight = n.kind === 'file' ? 400 : 600;
      return `<g class="nd ${n.kind} ${hot(n) ? '' : 'dim'} ${n.id === S.selected ? 'sel' : ''}" data-tnode="${esc(n.id)}" transform="translate(${n.x},${n.y})" tabindex="0" role="button" aria-label="${esc(label)}">
        <title>${esc(n.kind === 'file' ? n.id : comp.plain || label)}</title>
        <circle r="${r(n)}" fill="${fill}" stroke="${stroke}"/>
        <text x="${r(n) + 7}" y="${sub ? -1 : 4}" font-weight="${weight}">${esc(label)}</text>
        ${sub ? `<text class="sub" x="${r(n) + 7}" y="12">${esc(sub)}</text>` : ''}
      </g>`;
    }).join('');
    $('#tree2d').innerHTML = `<svg width="${W}" height="${Math.max(H, 200)}" viewBox="0 0 ${W} ${Math.max(H, 200)}" style="margin-top:56px" role="img" aria-label="Project tree">${edges}${xl}${nodes}</svg>`;
  }
  document.addEventListener('click', (e) => {
    const g = e.target.closest('#tree2d [data-tnode]'); if (g) { treeClick(g.dataset.tnode); return; }
    if (e.target.closest('#clearTreeHi')) { S.tree.highlight = null; renderTree(true); }
  });
  document.addEventListener('keydown', (e) => {
    if ((e.key === 'Enter' || e.key === ' ') && e.target.matches && e.target.matches('#tree2d [data-tnode]')) { e.preventDefault(); treeClick(e.target.dataset.tnode); }
  });

  function press(attr, value) { $$(`[${attr}]`).forEach((b) => b.setAttribute('aria-pressed', String(b.getAttribute(attr) === String(value)))); }
  $$('[data-tdim]').forEach((b) => b.addEventListener('click', () => { S.tree.dims = Number(b.dataset.tdim); renderTree(true); }));
  $('#treeOpenAll').addEventListener('click', () => { for (const [id, n] of T.nodes) if (n.kind !== 'file') S.tree.open.add(id); renderTree(true); });
  $('#treeCloseAll').addEventListener('click', () => { S.tree.open = new Set(['root']); renderTree(true); });
  $('#treeLinks').addEventListener('change', (e) => { S.tree.links = e.target.checked; renderTree(false); });
  $('#treeFit').addEventListener('click', treeFit);

  // =====================================================================
  // History: every commit in plain words, with the design at any version
  // =====================================================================
  const KIND_NAMES = { feature: 'New', fix: 'Fix', cleanup: 'Clean-up', tests: 'Tests', docs: 'Docs', setup: 'Setup', change: 'Change', save: 'Saved' };
  const FILTERS = [['all', 'Everything'], ['feature', 'New things'], ['fix', 'Fixes'], ['cleanup', 'Clean-ups'], ['tests', 'Tests'], ['setup', 'Setup']];

  async function renderHistory() {
    const v = $('#view-history');
    if (!S.hist) {
      v.innerHTML = '<div class="page"><p class="muted">Reading the history…</p></div>';
      try { S.hist = await api('/api/timeline'); } catch (e) { v.innerHTML = `<div class="page"><p class="msg error">${esc(e.message)}</p></div>`; return; }
      if (S.view !== 'history') return;
    }
    const h = S.hist;
    const q = S.histQuery.trim().toLowerCase();
    const match = (c) => (S.histFilter === 'all' || c.kind === S.histFilter)
      && (!S.histPart || (c.comps || []).some((x) => x.id === S.histPart))
      && (!q || c.subject.toLowerCase().includes(q) || (c.files || []).some((f) => f.path.toLowerCase().includes(q)) || (c.author || '').toLowerCase().includes(q));

    if (!h.git) {
      v.innerHTML = `<div class="page narrow">
        <div class="callout"><h3>This folder does not use Git</h3><p>CodeBase keeps its own save points while it is open: every time files change, it notes what changed and which parts of the design were touched. Open a GitHub link, or run <span class="mono">git init</span> in the folder, to see the full history from the first version.</p></div>
        <div class="section"><div class="section-head"><h2>Save points</h2><span class="muted">${plural(h.journal.length, 'save point')}</span></div>
        ${h.journal.length ? h.journal.map((j, i) => commitCard({ ...j, hash: 'j' + i, subject: j.summary, kind: 'save' }, false)).join('') : '<p class="muted">None yet. Edit a file while CodeBase is open.</p>'}</div></div>`;
      return;
    }
    const shown = h.commits.filter(match);
    const parts = new Map();
    for (const c of h.commits) for (const x of c.comps || []) parts.set(x.id, x.name);
    const first = h.commits[h.commits.length - 1], last = h.commits[0];
    const tags = h.commits.flatMap((c) => c.tags || []);
    let html = '', day = '';
    for (const c of shown) {
      for (const t of c.tags || []) html += `<div class="version">Version ${esc(t)}</div>`;
      const d = new Date(c.at).toLocaleDateString(undefined, { weekday: 'short', year: 'numeric', month: 'short', day: 'numeric' });
      if (d !== day) { if (day) html += '</div>'; html += `<div class="day"><h3>${esc(d)}</h3>`; day = d; }
      html += commitCard(c, true);
    }
    if (day) html += '</div>';

    v.innerHTML = `<div class="page">
      <div class="hist-head"><h2>${plural(h.total, 'commit')}</h2>
        <span class="muted">${h.branch ? `on ${esc(h.branch)}` : ''}${first ? `, from ${new Date(first.at).toLocaleDateString()} to ${ago(last.at)}` : ''}${tags.length ? `, ${plural(tags.length, 'tagged version')}` : ''}</span></div>
      ${h.shallow ? '<div class="callout warn" style="margin-bottom:16px"><h3>Only part of the history is here</h3><p>This copy was downloaded with recent commits only. Open the link again from CodeBase to download the rest.</p></div>' : ''}
      ${h.commits.length < h.total ? `<p class="muted small-note" style="margin:-8px 0 14px">Showing the newest ${h.commits.length}.</p>` : ''}
      ${activityBars(h.commits)}
      ${h.pending ? `<div class="commit" style="border-color:var(--warn)"><button data-hist-toggle="pending"><span class="kindtag" style="color:var(--warn)">Not saved</span><span class="subj">Changes not committed to Git yet</span><span class="meta">${plural(h.pending.files.length, 'file')}</span><span class="sum">${esc(h.pending.summary)}</span></button>
        ${S.histOpen.has('pending') ? `<div class="body">${filesList(h.pending.files, null)}</div>` : ''}</div>` : ''}
      <div class="hist-filters">
        <div class="seg" role="group" aria-label="Kind of change">${FILTERS.map(([k, l]) => `<button data-hist-filter="${k}" aria-pressed="${S.histFilter === k}">${l}</button>`).join('')}</div>
        <select id="histPart" style="max-width:220px" aria-label="Part of the design"><option value="">Every part</option>${[...parts].sort((a, b) => a[1].localeCompare(b[1])).map(([id, n]) => `<option value="${esc(id)}" ${S.histPart === id ? 'selected' : ''}>${esc(n)}</option>`).join('')}</select>
        <input class="field" id="histQuery" placeholder="Search messages, files, people" value="${esc(S.histQuery)}">
      </div>
      ${shown.length ? html : '<p class="muted">No commits match.</p>'}
    </div>`;
  }

  function activityBars(commits) {
    if (commits.length < 4) return '';
    const months = new Map();
    for (const c of commits) { const d = new Date(c.at); const k = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}`; months.set(k, (months.get(k) || 0) + 1); }
    const keys = [...months.keys()].sort();
    const [y0, m0] = keys[0].split('-').map(Number), [y1, m1] = keys[keys.length - 1].split('-').map(Number);
    const all = [];
    for (let y = y0, m = m0; y < y1 || (y === y1 && m <= m1); m === 12 ? (y++, m = 1) : m++) all.push(`${y}-${String(m).padStart(2, '0')}`);
    if (all.length < 2) return '';
    const max = Math.max(...months.values());
    const bw = Math.max(4, Math.min(18, Math.floor(900 / all.length) - 2));
    const W = all.length * (bw + 2), H = 46;
    return `<div style="margin:0 0 18px;overflow-x:auto"><svg width="${W}" height="${H + 16}" role="img" aria-label="Commits per month">
      ${all.map((k, i) => { const n = months.get(k) || 0; const hh = n ? Math.max(2, (n / max) * H) : 1; return `<rect x="${i * (bw + 2)}" y="${H - hh}" width="${bw}" height="${hh}" rx="1.5" fill="${n ? 'var(--accent)' : 'var(--line)'}"><title>${k}: ${plural(n, 'commit')}</title></rect>`; }).join('')}
      <text x="0" y="${H + 13}" font-size="11" fill="var(--text-3)">${all[0]}</text><text x="${W}" y="${H + 13}" font-size="11" fill="var(--text-3)" text-anchor="end">${all[all.length - 1]}</text></svg></div>`;
  }

  function filesList(list, hash) {
    return `<ul class="fchanges">${list.slice(0, 120).map((f) => `<li><span class="st ${f.status}">${f.status === 'added' ? 'New' : f.status === 'removed' ? 'Removed' : 'Changed'}</span>
      <span>${S.byId.has(f.path) ? `<button class="link path" data-file="${esc(f.path)}">${esc(f.path)}</button>` : `<span class="path">${esc(f.path)}</span>`} <span class="faint" style="font-size:12px">${esc(compInfo(f.comp).name)}</span></span>
      <span>${hash ? `<button class="btn ghost small" data-diff="${esc(hash)}" data-diff-path="${esc(f.path)}">See change${f.add || f.del ? ` +${f.add} −${f.del}` : ''}</button>` : ''}</span></li>`).join('')}
      ${list.length > 120 ? `<li><span></span><span class="muted">and ${list.length - 120} more</span></li>` : ''}</ul>`;
  }

  function commitCard(c, git) {
    const open = S.histOpen.has(c.hash);
    return `<div class="commit"><button data-hist-toggle="${esc(c.hash)}" aria-expanded="${open}">
      <span class="kindtag k-${esc(c.kind)}">${esc(KIND_NAMES[c.kind] || 'Change')}</span>
      <span class="subj">${esc(c.subject)}${(c.tags || []).length ? `<span class="tagline">${c.tags.map((t) => `<span class="chip key">${esc(t)}</span>`).join('')}</span>` : ''}</span>
      <span class="meta">${git ? `${esc(c.author)}<br>${new Date(c.at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}<br><span class="plus">+${c.add}</span> <span class="minus">−${c.del}</span>` : ago(c.at)}</span>
      <span class="sum">${esc(c.summary)}</span></button>
      ${open ? `<div class="body">
        <div class="chips" style="margin:10px 0">${(c.comps || []).map((x) => `<button class="chip" data-part="${esc(x.id)}"><i style="background:${famVar(x.id)}"></i>${esc(x.name)} ${x.files}</button>`).join('')}</div>
        ${git ? `<div class="actions"><button class="btn small" data-compare="${esc(c.hash)}">Compare the design then and now</button><button class="btn ghost small" data-copy="${esc(c.hash)}">Copy commit id ${esc(c.short)}</button></div><div id="cmp-${esc(c.hash)}"></div>` : ''}
        ${filesList(c.files || [], git ? c.hash : null)}
      </div>` : ''}</div>`;
  }

  async function compareAt(rev) {
    const box = document.getElementById('cmp-' + rev); if (!box) return;
    box.innerHTML = '<p class="muted">Reading that version…</p>';
    try {
      const r = await api('/api/at?rev=' + encodeURIComponent(rev));
      const rows = r.components.filter((c) => c.then || c.now);
      box.innerHTML = `<p class="muted small-note">${plural(r.filesThen, 'file')} then, ${plural(r.filesNow, 'file')} now.</p>
        <table class="compare"><thead><tr><th>Part</th><th class="num">Then</th><th class="num">Now</th><th>Added since</th><th>Removed since</th></tr></thead><tbody>
        ${rows.map((c) => { const d = c.now - c.then; return `<tr><td><span class="dot" style="background:${famVar(c.id)}"></span> ${esc(c.name)}</td><td class="num">${c.then}</td><td class="num ${d > 0 ? 'up' : d < 0 ? 'down' : ''}">${c.now}</td>
          <td>${c.added.slice(0, 4).map((p) => `<span class="path">${esc(base(p))}</span>`).join(', ')}${c.added.length > 4 ? ` and ${c.added.length - 4} more` : ''}</td>
          <td>${c.removed.slice(0, 4).map((p) => `<span class="path">${esc(base(p))}</span>`).join(', ')}${c.removed.length > 4 ? ` and ${c.removed.length - 4} more` : ''}</td></tr>`; }).join('')}</tbody></table>`;
    } catch (e) { box.innerHTML = `<p class="msg error">${esc(e.message)}</p>`; }
  }

  async function showDiff(rev, path) {
    const d = $('#diffDialog');
    $('#diffTitle').innerHTML = `<b class="path">${esc(path)}</b><p class="muted small-note">Loading…</p>`;
    $('#diffBody').textContent = '';
    d.showModal();
    try {
      const r = await api(`/api/diff?rev=${encodeURIComponent(rev)}&path=${encodeURIComponent(path)}`);
      $('#diffTitle').innerHTML = `<b class="path">${esc(path)}</b><p class="muted small-note">${esc(r.subject || '')}, ${esc(r.author || '')}, ${r.at ? new Date(r.at).toLocaleString() : ''}</p>`;
      const lines = (r.patch || 'No text changes (the file may be binary).').split('\n');
      $('#diffBody').innerHTML = lines.map((l) => {
        const cls = l.startsWith('+++') || l.startsWith('---') || l.startsWith('diff ') || l.startsWith('index ') ? 'h' : l.startsWith('@@') ? 'h' : l.startsWith('+') ? 'a' : l.startsWith('-') ? 'd' : '';
        return cls ? `<span class="${cls}">${esc(l)}</span>` : esc(l) + '\n';
      }).join('') + (r.truncated ? '\n… cut here, the change is very large.' : '');
    } catch (e) { $('#diffBody').textContent = e.message; }
  }
  $('#diffClose').addEventListener('click', () => $('#diffDialog').close());

  document.addEventListener('click', (e) => {
    const t = e.target.closest('[data-hist-toggle]');
    if (t) { const k = t.dataset.histToggle; if (S.histOpen.has(k)) S.histOpen.delete(k); else S.histOpen.add(k); renderHistory(); return; }
    const f = e.target.closest('[data-hist-filter]');
    if (f) { S.histFilter = f.dataset.histFilter; renderHistory(); return; }
    const c = e.target.closest('[data-compare]');
    if (c) { compareAt(c.dataset.compare); return; }
    const d = e.target.closest('[data-diff]');
    if (d) showDiff(d.dataset.diff, d.dataset.diffPath);
  });
  let histTimer;
  document.addEventListener('input', (e) => {
    if (e.target.id === 'histQuery') { S.histQuery = e.target.value; clearTimeout(histTimer); histTimer = setTimeout(() => { renderHistory().then(() => { const i = $('#histQuery'); if (i) { i.focus(); i.setSelectionRange(i.value.length, i.value.length); } }); }, 250); }
  });
  document.addEventListener('change', (e) => { if (e.target.id === 'histPart') { S.histPart = e.target.value; renderHistory(); } });
  // =====================================================================
  // Ask AI: the context-pack builder (server: src/context.rs)
  // =====================================================================
  const BUDGETS = [
    [4000, '4k', 'Small free chats'],
    [8000, '8k', 'Free ChatGPT, Claude, Gemini'],
    [16000, '16k', 'Most paid chats'],
    [32000, '32k', 'Long-context models'],
  ];
  const MODES = [['full', 'Full code'], ['excerpt', 'Part'], ['outline', 'Outline'], ['summary', 'Summary'], ['off', 'Off']];
  const MODE_HELP = {
    full: 'The whole file. The AI may change it.',
    excerpt: 'Only the lines around the error. The AI may change it.',
    outline: 'Only names of classes and functions, so the AI knows it exists.',
    summary: 'One line saying what it does.',
    off: 'Left out.',
  };
  S.pack = null;
  S.packModes = new Map();   // path -> mode the user picked
  S.packBudget = 8000;
  S.packPlain = true;
  S.packSeeds = [];
  S.packSeedReason = '';

  function packRequest() {
    return {
      task: S.askGoal || '', done: S.askDoneText || '', budget: S.packBudget, plain: S.packPlain,
      pins: [...S.ask], seeds: S.packSeeds, seedReason: S.packSeedReason, modes: Object.fromEntries(S.packModes),
    };
  }

  let packSeq = 0;
  async function buildPack() {
    const seq = ++packSeq;
    const msg = $('#askMsg');
    if (msg) { msg.className = 'msg'; msg.textContent = 'Picking files…'; }
    try {
      const pack = await api('/api/context', packRequest());
      if (seq !== packSeq) return null;
      S.pack = pack;
      if (msg) msg.textContent = '';
      renderPack();
      return pack;
    } catch (e) { if (msg) { msg.className = 'msg error'; msg.textContent = e.message; } return null; }
  }
  let packTimer;
  const rebuildSoon = () => { clearTimeout(packTimer); packTimer = setTimeout(buildPack, 250); };

  function renderAsk() {
    const hasInput = (S.askGoal || '').trim() || S.ask.size || S.packSeeds.length;
    $('#view-ask').innerHTML = `<div class="page narrow ask-page">
      <p class="lede">Describe a change or paste an error. CodeBase picks the files that matter, follows how they connect, and fits them to your AI chat's size. Nothing leaves this computer until you paste it.</p>

      <div class="ask-input">
        <label for="askGoal" class="field-label">What do you want to do?</label>
        <textarea id="askGoal" rows="5" placeholder="Add a 'paid on' date to bills and show it in the bill list

or paste an error message, for example:
  at MyApp.Services.BillService.Total(Bill bill) in BillService.cs:line 9">${esc(S.askGoal || '')}</textarea>
        ${S.packSeeds.length ? `<p class="seed-note">Starting from <b>${esc(S.packSeedReason || plural(S.packSeeds.length, 'file'))}</b> <button class="link" id="clearSeeds">Clear</button></p>` : ''}
        <details class="more-opts" ${S.askDoneText ? 'open' : ''}><summary>How will you know it worked? (optional)</summary>
          <textarea id="askDone" rows="2" placeholder="I can pick a date when saving a bill, and it shows in the list">${esc(S.askDoneText || '')}</textarea></details>
      </div>

      <div class="ask-controls">
        <div>
          <span class="field-label">How much can your AI chat read?</span>
          <div class="budget" role="radiogroup" aria-label="Size budget">${BUDGETS.map(([n, l, hint]) => `<button role="radio" aria-checked="${S.packBudget === n}" data-budget="${n}" title="${esc(hint)}"><b>${l}</b><small>${esc(hint)}</small></button>`).join('')}</div>
        </div>
        <label class="check"><input type="checkbox" id="askPlain" ${S.packPlain ? 'checked' : ''}> I am not a programmer: ask for plain words and step-by-step instructions</label>
        <div class="actions" style="margin:0"><button class="btn primary" id="askBuild">${hasInput ? 'Build context pack' : 'Build context pack'}</button><span id="askMsg" class="msg"></span></div>
      </div>

      <div id="packOut"></div>
    </div>`;
    if (S.pack) renderPack();
  }

  function modeSelect(it) {
    const opts = MODES.filter(([m]) => m !== 'excerpt' || it.mode === 'excerpt' || (S.pack.trace || []).some((t) => t.path === it.path));
    return `<select class="mode-sel mode-${it.mode}" data-pack-mode="${esc(it.path)}" aria-label="How to include ${esc(it.path)}" title="${esc(MODE_HELP[it.mode])}">
      ${opts.map(([m, l]) => `<option value="${m}" ${m === it.mode ? 'selected' : ''}>${l}${m === 'full' && it.mode !== 'full' && it.fullTokens ? ` (~${it.fullTokens.toLocaleString()})` : ''}</option>`).join('')}</select>`;
  }

  function renderPack() {
    const out = $('#packOut'); const p = S.pack;
    if (!out || !p) return;
    const pct = Math.min(100, Math.round((p.tokens / p.budget) * 100));
    const groups = [
      ['Full code', 'The AI may change these', (i) => i.mode === 'full' || i.mode === 'excerpt'],
      ['For reference', 'Outlines and one-line summaries', (i) => i.mode === 'outline' || i.mode === 'summary'],
      ['Left out', 'Turn on anything that matters', (i) => i.mode === 'off'],
    ];
    const row = (it) => {
      const n = S.byId.get(it.path) || {};
      const c = compInfo(n.comp);
      return `<li class="pack-row ${it.mode}">
        <span class="dot" style="background:${famVar(n.comp)}" title="${esc(c.name)}"></span>
        <div class="pack-main">
          <button class="link mono" data-file="${esc(it.path)}">${esc(it.path)}</button>
          <div class="why">${esc(it.reasons.join(' · ') || it.summary)}</div>
        </div>
        <span class="tok mono">${it.tokens ? it.tokens.toLocaleString() : ''}</span>
        ${modeSelect(it)}
        ${!it.auto ? `<button class="btn ghost small" data-pack-reset="${esc(it.path)}" title="Let CodeBase decide">Auto</button>` : '<span class="auto-slot"></span>'}
      </li>`;
    };
    out.innerHTML = `<section class="pack">
      <div class="pack-head">
        <div class="meter ${p.overBudget ? 'over' : ''}" role="img" aria-label="${pct}% of the budget used">
          <i style="width:${pct}%"></i><span class="meter-brief" style="width:${Math.min(100, (p.briefTokens / p.budget) * 100)}%" title="Project brief"></span>
        </div>
        <div class="pack-sum"><b>${p.tokens.toLocaleString()}</b> of ${p.budget.toLocaleString()} tokens <span class="muted">· ${plural(p.items.filter((i) => i.mode !== 'off').length, 'file')} · brief ${p.briefTokens}</span></div>
      </div>
      ${p.notes.length ? `<ul class="pack-notes">${p.notes.map((n) => `<li>${esc(n)}</li>`).join('')}</ul>` : ''}
      ${p.items.length ? groups.map(([title, sub, f]) => { const list = p.items.filter(f); return list.length ? `<h3 class="pack-group">${title} <span class="muted">${sub}</span></h3><ul class="pack-list">${list.map(row).join('')}</ul>` : ''; }).join('')
        : '<p class="muted">No files picked yet. Describe the change, paste an error, or add a file below.</p>'}
      <div class="row-form"><input class="field" id="askAdd" placeholder="Add another file by name" list="askAddList" autocomplete="off"><button class="btn" id="askAddBtn">Add</button></div>
      <datalist id="askAddList">${files().slice(0, 2000).map((n) => `<option value="${esc(n.id)}">`).join('')}</datalist>
      <div class="out">
        <div class="out-head"><span class="muted" style="font-size:13px">Paste this as your first message in a new chat.</span>
          <span class="actions" style="margin:0"><button class="btn small" id="askDownload">Download .md</button><button class="btn primary" id="askCopy">Copy context pack</button></span></div>
        <textarea id="askText" class="code-area" spellcheck="false" readonly aria-label="Context pack">${esc(p.text)}</textarea>
      </div>
    </section>`;
  }

  /** "Copy context for this": build a pack around files and copy it straight away. */
  async function copyContextFor(seeds, reason) {
    S.packSeeds = seeds; S.packSeedReason = reason; S.packModes.clear();
    toast('Building context…');
    const pack = await buildPack();
    if (!pack) { toast('Could not build the context pack'); return; }
    const ok = await copyText(pack.text);
    toast(ok ? `Copied about ${pack.tokens.toLocaleString()} tokens. Add your request at the top of the chat.` : 'Opened in Ask AI. Press Copy there.');
    if (!ok) go('ask');
  }
  function folderSeeds(id) {
    const prefix = id === '.' ? '' : id + '/';
    return files().filter((f) => f.id.startsWith(prefix) && !f.generated).sort((a, b) => (b.importance || 0) - (a.importance || 0)).slice(0, 10).map((f) => f.id);
  }

  document.addEventListener('input', (e) => {
    if (e.target.id === 'askGoal') S.askGoal = e.target.value;
    if (e.target.id === 'askDone') S.askDoneText = e.target.value;
  });
  document.addEventListener('change', (e) => {
    if (e.target.id === 'askPlain') { S.packPlain = e.target.checked; if (S.pack) rebuildSoon(); }
    const m = e.target.closest('[data-pack-mode]');
    if (m) { S.packModes.set(m.dataset.packMode, m.value); rebuildSoon(); }
  });
  document.addEventListener('keydown', (e) => {
    if (e.target.id === 'askGoal' && e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); buildPack(); }
    if (e.key === 'Enter' && e.target.id === 'askAdd') { e.preventDefault(); $('#askAddBtn')?.click(); }
  });
  document.addEventListener('click', async (e) => {
    const b = e.target.closest('[data-budget]');
    if (b) { S.packBudget = Number(b.dataset.budget); $$('[data-budget]').forEach((x) => x.setAttribute('aria-checked', String(x === b))); if (S.pack) buildPack(); return; }
    const r = e.target.closest('[data-pack-reset]');
    if (r) { S.packModes.delete(r.dataset.packReset); buildPack(); return; }
    if (e.target.closest('#clearSeeds')) { S.packSeeds = []; S.packSeedReason = ''; renderAsk(); return; }
    if (e.target.closest('#askBuild')) {
      if (!(S.askGoal || '').trim() && !S.ask.size && !S.packSeeds.length) {
        $('#askMsg').className = 'msg error'; $('#askMsg').textContent = 'Describe the change or paste an error first.'; $('#askGoal').focus(); return;
      }
      S.packModes.clear(); buildPack(); return;
    }
    if (e.target.closest('#askAddBtn')) {
      const v = $('#askAdd').value.trim();
      const n = S.byId.get(v) || files().find((x) => x.name.toLowerCase() === v.toLowerCase());
      if (!n) { toast('No file with that name'); return; }
      S.packModes.set(n.id, 'full'); $('#askAdd').value = ''; buildPack(); return;
    }
    if (e.target.closest('#askCopy')) {
      await copyText(S.pack.text);
      toast('Copied. Paste it into your AI chat.');
      const used = S.pack.items.filter((i) => i.mode === 'full' || i.mode === 'excerpt').map((i) => i.path);
      api('/api/history', { paths: used, task: S.askGoal || S.packSeedReason || '' }).catch(() => {});
    }
    if (e.target.closest('#askDownload')) {
      const a = document.createElement('a');
      a.href = URL.createObjectURL(new Blob([S.pack.text], { type: 'text/markdown' }));
      a.download = `${S.meta.name}-context.md`; a.click(); setTimeout(() => URL.revokeObjectURL(a.href), 1000);
    }
    const cf = e.target.closest('[data-copy-context]');
    if (cf) {
      const id = cf.dataset.copyContext; const n = S.byId.get(id);
      if (n && n.type === 'dir') copyContextFor(folderSeeds(id), `folder ${id === '.' ? S.meta.name : id + '/'}`);
      else copyContextFor([id], id.split('/').pop());
    }
    const fc = e.target.closest('[data-flow-context]');
    if (fc) { const f = S.flowById.get(fc.dataset.flowContext); if (f) copyContextFor(flowSeeds(f), `the flow "${f.title}"`); }
    const pc = e.target.closest('[data-part-context]');
    if (pc) { const id = pc.dataset.partContext; copyContextFor(partSeeds(id), `the ${compInfo(id).name.toLowerCase()} part`); }
  });

  // =====================================================================
  // Memory
  // =====================================================================
  function codeFiles() { return files().filter((n) => !n.generated && !n.large); }

  function coverageHtml() {
    const fl = codeFiles();
    const mine = fl.filter((n) => n.note && n.noteSource === 'you').length;
    const ai = fl.filter((n) => n.note && n.noteSource === 'ai').length;
    const stale = fl.filter((n) => n.noteStale).length;
    const pct = (x) => (fl.length ? (x / fl.length) * 100 : 0);
    return `<div class="coverage">
        <div><b>${mine}</b><span>described by you</span></div>
        <div><b>${ai}</b><span>described by AI</span></div>
        <div><b>${fl.length - mine - ai}</b><span>auto summary only</span></div>
        <div><b>${stale}</b><span>may be outdated</span></div>
      </div>
      <div class="bar" aria-hidden="true"><i style="width:${pct(mine)}%;background:var(--accent)"></i><i style="width:${pct(ai)}%;background:var(--ok)"></i></div>`;
  }

  function renderMemory() {
    const parts = (S.arch.components || []).filter((c) => c.count > 0);
    const scope = S.memScope || 'areas';
    $('#view-memory').innerHTML = `<div class="page narrow">
      <p class="muted" style="max-width:64ch;margin:0">CodeBase writes a quick summary for every file on its own. Plain descriptions from you or an AI chat are better: they power the overview, the flows and every prompt. They are saved in <span class="mono">.codebase/memory.json</span> inside your project.</p>
      <div id="memCoverage">${coverageHtml()}</div>

      <ol class="steps">
        <li><div><h3>Choose what to describe</h3>
          <div class="row-form">
            <select id="memScope" aria-label="What to describe">
              <option value="areas" ${scope === 'areas' ? 'selected' : ''}>The project and its main folders (best first step)</option>
              <option value="missing" ${scope === 'missing' ? 'selected' : ''}>Files without a description</option>
              <option value="stale" ${scope === 'stale' ? 'selected' : ''}>Descriptions that may be outdated</option>
              ${(S.journeys || []).map((j) => `<option value="journey:${esc(j.id)}" ${scope === 'journey:' + j.id ? 'selected' : ''}>Files in the ${esc(j.name)} journey</option>`).join('')}
              ${parts.map((c) => `<option value="part:${esc(c.id)}" ${scope === 'part:' + c.id ? 'selected' : ''}>${esc(c.name)} (${c.count})</option>`).join('')}
            </select>
            <select id="memSize" aria-label="How many files" style="max-width:130px"><option value="15">15 files</option><option value="30" selected>30 files</option><option value="60">60 files</option></select>
          </div>
          <div class="actions"><button class="btn primary" id="memBuild">Build the prompt</button><span id="memMsg" class="msg"></span></div></div></li>
        <li><div><h3>Paste it into any AI chat</h3><p class="hint">ChatGPT, Claude, Gemini, DeepSeek: any of them works.</p>
          <div class="out" id="memOut" hidden><div class="out-head"><span class="muted mono" id="memTokens" style="font-size:12px"></span><button class="btn primary small" id="memCopy">Copy prompt</button></div>
          <textarea id="memText" class="code-area" spellcheck="false" style="min-height:200px" aria-label="Describe prompt"></textarea></div></div></li>
        <li><div><h3>Paste the answer back</h3>
          <textarea id="memReply" class="code-area" rows="6" placeholder='{"app/Models/Bill.php": "A contractor bill submitted against a works package."}'></textarea>
          <div class="actions"><button class="btn primary" id="memSave">Save descriptions</button><span id="memSaveMsg" class="msg"></span></div></div></li>
      </ol></div>`;
  }

  function describeAreas() {
    const folders = [...S.byId.values()].filter((n) => n.type === 'dir' && n.id !== '.' && n.depth <= 2 && (n.fileCount || 0) > 0).sort((a, b) => a.id.localeCompare(b.id)).slice(0, 40);
    const blocks = folders.map((d) => {
      const inside = files().filter((n) => n.id.startsWith(d.id + '/')).slice(0, 20);
      return `<folder path="${d.id}/">\n${inside.map((n) => `- ${n.id.slice(d.id.length + 1)}: ${n.note || n.summary || ''}`).join('\n')}${(d.fileCount || 0) > 20 ? `\n- …and ${d.fileCount - 20} more files` : ''}\n</folder>`;
    });
    const top = codeFiles().filter((n) => !n.id.includes('/')).map((n) => `- ${n.id}: ${n.note || n.summary || ''}`).join('\n');
    const text = `<role>
You are a senior engineer writing a short, plain-language map of a codebase for someone who is not a programmer.
</role>

<project name="${S.meta.name}" built_with="${S.meta.stack.join(', ')}">
${S.meta.description ? `README says: ${S.meta.description}\n` : ''}Top-level files:
${top || '(none)'}
</project>

<task>
1. Write ONE plain sentence (max 25 words) saying what the whole project is and who uses it. Use the key ".".
2. For each folder below, write ONE plain sentence (max 20 words) saying what that folder is for in this project. Use the folder path exactly as given, including the trailing slash.
Describe purpose in everyday words, not file types or jargon.
</task>

${blocks.join('\n\n')}

<answer_format>
Return ONLY a JSON object, nothing before or after it:
{".": "…", "src/": "…", "src/api/": "…"}
</answer_format>
`;
    return { text, label: `the project and ${plural(folders.length, 'folder')}` };
  }

  async function describeFiles(scope, size) {
    let pool = codeFiles();
    if (scope === 'missing') pool = pool.filter((n) => !n.note);
    else if (scope === 'stale') pool = pool.filter((n) => n.noteStale);
    else if (scope.startsWith('journey:')) { const set = new Set(journeyFiles(scope.slice(8))); pool = pool.filter((n) => set.has(n.id)); }
    else if (scope.startsWith('part:')) { const c = scope.slice(5); pool = pool.filter((n) => n.comp === c); }
    pool.sort((a, b) => (b.inDeg - a.inDeg) || (b.lines - a.lines));
    const batch = pool.slice(0, size);
    if (!batch.length) return null;
    const got = await api('/api/files', { paths: batch.map((n) => n.id) });
    const blocks = got.map((f) => {
      const head = (f.content || '').split('\n').slice(0, 80).join('\n');
      const sigs = f.symbols.slice(0, 25).map((s) => s.sig).join('\n');
      return `<file path="${f.path}">\n${sigs ? `Definitions:\n${sigs}\n\n` : ''}First lines:\n${head}\n</file>`;
    });
    const text = `<role>
You are a senior engineer writing a plain-language map of a codebase for someone who is not a programmer.
</role>

<project>
${S.meta.name} (built with ${S.meta.stack.join(', ') || 'unknown'})${S.meta.description ? `\n${S.meta.description}` : ''}
</project>

<task>
For each file below, write ONE plain sentence (max 25 words) saying what the file does for the app and its users. Name the key class or function only if it helps. No syntax descriptions, no lists of every function.
</task>

${blocks.join('\n\n')}

<answer_format>
Return ONLY a JSON object mapping each exact path to its sentence, nothing before or after it:
{"path/one.php": "…", "path/two.js": "…"}
</answer_format>
`;
    return { text, label: plural(batch.length, 'file') };
  }

  function parseReply(raw) {
    const start = raw.indexOf('{'), end = raw.lastIndexOf('}');
    if (start !== -1 && end > start) { try { return JSON.parse(raw.slice(start, end + 1)); } catch { /* fall through */ } }
    const out = {};
    for (const line of raw.split('\n')) {
      const m = line.match(/^[\s\-*]*`?([\w./\\\-@]+(?:\.\w+|\/))`?\s*[:\-–]\s*(.+)$/);
      if (m) out[m[1].replace(/\\/g, '/').replace(/^\.\//, '')] = m[2].trim();
    }
    return out;
  }

  document.addEventListener('change', (e) => { if (e.target.id === 'memScope') S.memScope = e.target.value; });
  document.addEventListener('click', async (e) => {
    if (e.target.closest('#memBuild')) {
      const msg = $('#memMsg'); msg.className = 'msg'; msg.textContent = 'Preparing…';
      try {
        const scope = $('#memScope').value;
        const r = scope === 'areas' ? describeAreas() : await describeFiles(scope, Number($('#memSize').value));
        if (!r) { msg.textContent = 'Nothing to describe there.'; $('#memOut').hidden = true; return; }
        $('#memText').value = r.text; $('#memOut').hidden = false;
        $('#memTokens').textContent = `About ${tokens(r.text).toLocaleString()} tokens`;
        msg.textContent = `Ready: ${r.label}.`;
      } catch (err) { msg.className = 'msg error'; msg.textContent = err.message; }
    }
    if (e.target.closest('#memCopy')) { await copyText($('#memText').value); toast('Copied. Paste it into your AI chat.'); }
    if (e.target.closest('#memSave')) {
      const msg = $('#memSaveMsg');
      const notes = parseReply($('#memReply').value);
      if (!Object.keys(notes).length) { msg.className = 'msg error'; msg.textContent = 'No descriptions found. Paste the JSON the AI returned.'; return; }
      try {
        const r = await api('/api/notes', { notes });
        await poll();
        msg.className = 'msg'; msg.textContent = `Saved ${r.saved}.${r.unknown.length ? ` Skipped ${r.unknown.length} paths that are not in this project.` : ''}`;
        $('#memReply').value = '';
      } catch (err) { msg.className = 'msg error'; msg.textContent = err.message; }
    }
  });

  // =====================================================================
  // Clean up
  // =====================================================================
  async function addHidden(p) {
    try {
      await api('/api/hidden', { hidden: [...(S.meta.hidden || []), p] });
      if (S.selected && (S.selected === p || S.selected.startsWith(p.endsWith('/') ? p : p + '/') || S.selected + '/' === p)) closeInspector();
      S.ask.delete(p);
      await poll();
      toast(`Hidden ${p}. Show it again from Clean up.`);
    } catch (e) { toast(e.message); }
  }
  function renderCleanup() {
    const hidden = S.meta.hidden || [];
    const sugg = [];
    for (const n of S.byId.values()) {
      if (n.type === 'file' && n.generated) sugg.push([n.id, 'Looks generated or minified']);
      else if (n.large) sugg.push([n.id, 'Very large file']);
    }
    const mediaDirs = new Map();
    for (const n of S.byId.values()) if (n.type === 'asset' && n.id.includes('/')) { const d = n.id.slice(0, n.id.lastIndexOf('/')) + '/'; mediaDirs.set(d, (mediaDirs.get(d) || 0) + 1); }
    for (const [d, c] of mediaDirs) if (c >= 5) sugg.push([d, `${c} images, fonts or other media`]);
    $('#view-cleanup').innerHTML = `<div class="page narrow">
      <p class="muted" style="max-width:64ch;margin:0">CodeBase already skips dependencies, build output, caches, lock files and everything in your .gitignore. Hide anything else that adds noise to the map and to prompts. Your list is saved in <span class="mono">.codebase/config.json</span>.</p>
      <div class="section"><div class="section-head"><h2>Suggested</h2><span class="muted">Generated, minified or very large</span></div>
        <ul class="kv">${sugg.length ? sugg.slice(0, 40).map(([p, why]) => `<li><span>${esc(p)}<small>${esc(why)}</small></span><button class="btn small" data-hide="${esc(p)}">Hide</button></li>`).join('') : '<li class="none">Nothing to suggest. The map looks clean.</li>'}</ul></div>
      <div class="section"><div class="section-head"><h2>Hidden</h2><span class="muted">${plural(hidden.length, 'rule')}</span></div>
        <ul class="kv">${hidden.length ? hidden.map((h) => `<li><span>${esc(h)}</span><button class="btn ghost small" data-unhide="${esc(h)}">Show again</button></li>`).join('') : '<li class="none">Nothing hidden yet.</li>'}</ul>
        <div class="row-form"><input class="field mono" id="hidePattern" placeholder="public/assets/   or   *.generated.ts" aria-label="Pattern to hide"><button class="btn" id="hideAdd">Hide</button></div>
        <p class="muted" style="font-size:13px">Uses the same patterns as .gitignore: a trailing / means a folder, * matches any name.</p></div>
    </div>`;
  }
  document.addEventListener('click', async (e) => {
    const u = e.target.closest('[data-unhide]');
    if (u) { await api('/api/hidden', { hidden: (S.meta.hidden || []).filter((h) => h !== u.dataset.unhide) }); await poll(); toast('Shown again'); }
    if (e.target.closest('#hideAdd')) { const v = $('#hidePattern').value.trim(); if (v) addHidden(v); }
  });
  document.addEventListener('keydown', (e) => { if (e.key === 'Enter' && e.target.id === 'hidePattern') { const v = e.target.value.trim(); if (v) addHidden(v); } });


  // =====================================================================
  // Start
  // =====================================================================
  (async () => {
    readColors();
    $('#themeBtn').textContent = `Theme: ${document.documentElement.dataset.theme || 'system'}`;
    try { S.projects = await api('/api/projects'); $('#appVersion').textContent = `Version ${S.projects.version}`; } catch { /* offline */ }
    try { await loadGraph(); } catch { renderChrome(); renderNoProject(); }
    setInterval(poll, 1500);
  })();
})();
