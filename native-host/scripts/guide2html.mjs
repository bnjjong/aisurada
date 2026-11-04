// Markdown guide -> single-file HTML (no external resources).
// Usage: node scripts/guide2html.mjs <input.md> <output.html>   (from native-host; pnpm docs:guide)
import { readFileSync, writeFileSync } from 'node:fs';
import { marked } from 'marked';

const [src, out] = process.argv.slice(2);
const md = readFileSync(src, 'utf8');

const esc = s => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
const slugify = text => text.toLowerCase().replace(/<[^>]+>/g, '').replace(/&[a-z]+;/g, '')
  .replace(/[^\p{L}\p{N}\s-]/gu, '').trim().replace(/\s+/g, '-');

const headings = [];
const used = new Map();
let title = '', lead = '';

marked.use({
  gfm: true,
  renderer: {
    heading(token) {
      const inner = this.parser.parseInline(token.tokens);
      const plain = token.text.replace(/`/g, '');
      if (token.depth === 1) { title = plain; return ''; }
      let id = slugify(plain) || 'section';
      const n = used.get(id) ?? 0; used.set(id, n + 1);
      if (n) id += '-' + (n + 1);
      headings.push({ depth: token.depth, id, text: plain });
      return `<h${token.depth} id="${id}"><a class="anchor" href="#${id}" aria-label="이 절의 링크">#</a>${inner}</h${token.depth}>\n`;
    },
    code(token) {
      const lang = (token.lang || '').split(/\s+/)[0];
      const label = lang ? `<span class="code-lang">${esc(lang)}</span>` : '';
      return `<div class="code"><div class="code-bar">${label}<button type="button" class="copy" aria-label="코드 복사">복사</button></div><pre><code>${esc(token.text)}</code></pre></div>\n`;
    },
    blockquote(token) {
      return `<aside class="callout">${this.parser.parse(token.tokens)}</aside>\n`;
    },
    table(token) {
      // default table markup, wrapped so wide tables scroll inside their own box
      const head = token.header.map((c, i) => `<th${token.align[i] ? ` style="text-align:${token.align[i]}"` : ''}>${this.parser.parseInline(c.tokens)}</th>`).join('');
      const rows = token.rows.map(r => `<tr>${r.map((c, i) => `<td${token.align[i] ? ` style="text-align:${token.align[i]}"` : ''}>${this.parser.parseInline(c.tokens)}</td>`).join('')}</tr>`).join('\n');
      return `<div class="table-wrap"><table><thead><tr>${head}</tr></thead><tbody>\n${rows}\n</tbody></table></div>\n`;
    },
  },
});

let body = marked.parse(md);
// marked percent-encodes Korean fragments in links; the heading ids are raw, so decode them
body = body.replace(/href="#([^"]+)"/g, (_, h) => `href="#${decodeURIComponent(h)}"`);

// the paragraph right after the h1 is the lead text
body = body.replace(/^\s*<p>([\s\S]*?)<\/p>/, (_, p) => { lead = p; return ''; });

const toc = [];
for (const h of headings) {
  if (h.depth === 2) toc.push({ ...h, children: [] });
  else if (h.depth === 3 && toc.length) toc[toc.length - 1].children.push(h);
}
const tocHtml = toc.map(h => {
  const kids = h.children.length
    ? `<ul>${h.children.map(c => `<li><a href="#${c.id}">${esc(c.text)}</a></li>`).join('')}</ul>` : '';
  return `<li><a href="#${h.id}">${esc(h.text)}</a>${kids}</li>`;
}).join('\n');

const css = `
:root{
  --bg:#fbfbf9; --surface:#ffffff; --soft:#f3f3ef; --fg:#1d2024; --muted:#5d6570; --line:#dcdcd4;
  --accent:#1d5fd1; --accent-soft:#e8f0fd; --warn:#8a5a00; --warn-soft:#fff6e0; --code-bg:#f4f4ef;
  --radius:10px;
}
@media (prefers-color-scheme:dark){
  :root:not([data-theme="light"]){
    --bg:#121417; --surface:#181b1f; --soft:#1e2227; --fg:#e6e8eb; --muted:#9aa3ad; --line:#2f353c;
    --accent:#7fb0ff; --accent-soft:#18283f; --warn:#f0c26a; --warn-soft:#2a2210; --code-bg:#1a1e23;
  }
}
:root[data-theme="dark"]{
  --bg:#121417; --surface:#181b1f; --soft:#1e2227; --fg:#e6e8eb; --muted:#9aa3ad; --line:#2f353c;
  --accent:#7fb0ff; --accent-soft:#18283f; --warn:#f0c26a; --warn-soft:#2a2210; --code-bg:#1a1e23;
}
*{box-sizing:border-box}
html{scroll-behavior:smooth;scroll-padding-top:72px}
body{margin:0;background:var(--bg);color:var(--fg);
  font:16px/1.75 -apple-system,BlinkMacSystemFont,"Apple SD Gothic Neo","Noto Sans KR","Segoe UI",sans-serif;
  -webkit-text-size-adjust:100%}
a{color:var(--accent);text-decoration:none}a:hover{text-decoration:underline}
.topbar{position:sticky;top:0;z-index:20;background:color-mix(in srgb,var(--bg) 88%,transparent);
  backdrop-filter:blur(8px);border-bottom:1px solid var(--line)}
.topbar-in{max-width:1200px;margin:0 auto;padding:10px 16px;display:flex;align-items:center;gap:12px}
.brand{font-weight:700;letter-spacing:.2px;color:var(--fg)}
.topbar .spacer{flex:1}
.btn{font:inherit;font-size:13px;color:var(--fg);background:var(--surface);border:1px solid var(--line);
  border-radius:8px;padding:5px 11px;cursor:pointer}
.btn:hover{border-color:var(--accent);color:var(--accent)}
.layout{max-width:1200px;margin:0 auto;padding:0 16px;display:grid;grid-template-columns:260px minmax(0,1fr);gap:48px}
nav.toc{position:sticky;top:64px;align-self:start;max-height:calc(100vh - 80px);overflow:auto;padding:28px 0 28px;font-size:14px}
nav.toc ul{list-style:none;margin:0;padding:0}
nav.toc>ul>li{margin:2px 0}
nav.toc ul ul{margin:2px 0 6px 12px;border-left:1px solid var(--line);padding-left:10px}
nav.toc a{display:block;color:var(--muted);padding:3px 8px;border-radius:6px;line-height:1.45}
nav.toc a:hover{color:var(--fg);background:var(--soft);text-decoration:none}
nav.toc a.active{color:var(--accent);background:var(--accent-soft);font-weight:600}
main{min-width:0;padding:36px 0 96px}
header.hero{padding:8px 0 28px;border-bottom:1px solid var(--line);margin-bottom:8px}
.eyebrow{font-size:13px;color:var(--accent);font-weight:700;letter-spacing:.6px}
h1.title{font-size:clamp(28px,4.4vw,40px);line-height:1.25;margin:6px 0 14px;letter-spacing:-.4px}
.lead{color:var(--muted);margin:0;max-width:62ch}
.lead code{font-size:.9em}
h2{font-size:26px;line-height:1.35;margin:56px 0 14px;padding-top:6px;letter-spacing:-.2px}
h3{font-size:19px;line-height:1.4;margin:34px 0 8px}
h2 .anchor,h3 .anchor{margin-left:-1.1em;margin-right:.35em;color:var(--line);font-weight:400;opacity:0;transition:opacity .12s}
h2:hover .anchor,h3:hover .anchor{opacity:1}
.anchor:hover{text-decoration:none;color:var(--accent)}
p,li{max-width:78ch}
ul,ol{padding-left:1.4em}
li{margin:3px 0}
code{font:0.88em/1.5 ui-monospace,"SF Mono",SFMono-Regular,Menlo,Consolas,monospace;background:var(--code-bg);
  border:1px solid var(--line);border-radius:5px;padding:1px 5px;word-break:break-word}
.code{margin:14px 0;border:1px solid var(--line);border-radius:var(--radius);background:var(--code-bg);overflow:hidden}
.code-bar{display:flex;align-items:center;justify-content:space-between;padding:4px 8px 4px 14px;
  border-bottom:1px solid var(--line);background:var(--soft);min-height:32px}
.code-lang{font-size:12px;color:var(--muted);text-transform:lowercase}
.copy{font:inherit;font-size:12px;color:var(--muted);background:transparent;border:1px solid transparent;
  border-radius:6px;padding:2px 9px;cursor:pointer;margin-left:auto}
.copy:hover{color:var(--accent);border-color:var(--line);background:var(--surface)}
pre{margin:0;padding:14px 16px;overflow:auto;tab-size:2}
pre code{background:none;border:0;padding:0;font-size:13px;line-height:1.65;word-break:normal;white-space:pre}
.table-wrap{margin:16px 0;overflow-x:auto;border:1px solid var(--line);border-radius:var(--radius);background:var(--surface)}
table{border-collapse:collapse;width:100%;font-size:14.5px;line-height:1.6}
th,td{padding:9px 14px;text-align:left;vertical-align:top;border-bottom:1px solid var(--line)}
th{background:var(--soft);font-weight:650;white-space:nowrap;position:sticky;top:0}
tr:last-child td{border-bottom:0}
td code{white-space:normal;word-break:normal;overflow-wrap:normal}
.callout{margin:20px 0;padding:2px 18px;border:1px solid var(--line);border-left:4px solid var(--accent);
  border-radius:var(--radius);background:var(--accent-soft)}
.callout p{margin:12px 0}
strong{font-weight:650}
hr{border:0;border-top:1px solid var(--line);margin:32px 0}
footer.foot{margin-top:64px;padding-top:18px;border-top:1px solid var(--line);color:var(--muted);font-size:13px}
.totop{position:fixed;right:18px;bottom:18px;opacity:0;pointer-events:none;transition:opacity .15s}
.totop.show{opacity:1;pointer-events:auto}
@media (max-width:960px){
  .layout{grid-template-columns:minmax(0,1fr);gap:0}
  nav.toc{position:static;max-height:none;padding:16px 0 0;border-bottom:1px solid var(--line);margin-bottom:8px}
  nav.toc details>summary{cursor:pointer;color:var(--muted);padding:6px 0}
  main{padding-top:20px}
  h2 .anchor,h3 .anchor{display:none}
}
@media print{
  .topbar,.totop,.copy,nav.toc{display:none}
  .layout{display:block}
  body{background:#fff;color:#000}
  .code,.table-wrap{break-inside:avoid}
}
`;

const js = `
(function(){
  try{var t=localStorage.getItem('aisurada-guide-theme');if(t)document.documentElement.setAttribute('data-theme',t)}catch(e){}
  var tocD=document.getElementById("toc-d");
  if(tocD&&window.matchMedia("(min-width:961px)").matches)tocD.open=true;
  var themeBtn=document.getElementById("theme");
  if(themeBtn)themeBtn.addEventListener('click',function(){
    var cur=document.documentElement.getAttribute('data-theme');
    var dark=cur?cur==='dark':window.matchMedia('(prefers-color-scheme: dark)').matches;
    var next=dark?'light':'dark';
    document.documentElement.setAttribute('data-theme',next);
    try{localStorage.setItem('aisurada-guide-theme',next)}catch(e){}
  });
  document.querySelectorAll('.code').forEach(function(box){
    var btn=box.querySelector('.copy'),code=box.querySelector('code');
    if(!btn||!code)return;
    btn.addEventListener('click',function(){
      var text=code.textContent;
      var done=function(){btn.textContent='복사됨';setTimeout(function(){btn.textContent='복사'},1400)};
      if(navigator.clipboard&&navigator.clipboard.writeText){navigator.clipboard.writeText(text).then(done,function(){fallback(text,done)})}
      else fallback(text,done);
    });
  });
  function fallback(text,done){
    var ta=document.createElement('textarea');ta.value=text;ta.style.position='fixed';ta.style.opacity='0';
    document.body.appendChild(ta);ta.select();try{document.execCommand('copy');done()}catch(e){}
    document.body.removeChild(ta);
  }
  var links=[].slice.call(document.querySelectorAll('nav.toc a'));
  var map={};links.forEach(function(a){map[a.getAttribute('href').slice(1)]=a});
  var heads=[].slice.call(document.querySelectorAll('main h2, main h3'));
  if('IntersectionObserver' in window){
    var io=new IntersectionObserver(function(es){
      es.forEach(function(e){
        if(e.isIntersecting){
          links.forEach(function(a){a.classList.remove('active')});
          var a=map[e.target.id];if(a)a.classList.add('active');
        }
      });
    },{rootMargin:'-72px 0px -70% 0px',threshold:0});
    heads.forEach(function(h){io.observe(h)});
  }
  var top=document.getElementById('totop');
  window.addEventListener('scroll',function(){top.classList.toggle('show',window.scrollY>600)},{passive:true});
  top.addEventListener('click',function(){window.scrollTo({top:0})});
})();
`;

const html = `<!doctype html>
<html lang="ko">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>aisurada 개발자 가이드</title>
<meta name="description" content="aisurada 저장소를 빌드하고, 돌리고, 시험하고, 고치는 방법을 모은 개발자 사용 가이드">
<style>${css}</style>
</head>
<body>
<div class="topbar"><div class="topbar-in">
  <span class="brand">aisurada</span><span style="color:var(--muted)">개발자 가이드</span>
  <span class="spacer"></span>
  <button type="button" class="btn" id="theme" aria-label="밝게/어둡게 전환">밝게·어둡게</button>
</div></div>
<div class="layout">
  <nav class="toc" aria-label="목차">
    <details id="toc-d"><summary>목차</summary>
    <ul>
${tocHtml}
    </ul></details>
  </nav>
  <main>
    <header class="hero">
      <div class="eyebrow">DEVELOPER GUIDE</div>
      <h1 class="title">${esc(title)}</h1>
      <p class="lead">${lead}</p>
    </header>
${body}
    <footer class="foot">원본: <code>docs/developer-guide.md</code> · 이 HTML은 원본에서 만든 사본이라 원본을 고치면 다시 만들어야 합니다.</footer>
  </main>
</div>
<button type="button" class="btn totop" id="totop" aria-label="맨 위로">↑ 맨 위로</button>
<script>${js}</script>
</body>
</html>
`;

writeFileSync(out, html);
console.log(`wrote ${out}: ${(html.length / 1024).toFixed(1)} KiB, h2 ${toc.length}, h3 ${headings.filter(h => h.depth === 3).length}`);
