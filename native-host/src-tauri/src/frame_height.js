(function () {
  let last = -1, timer = 0;
  function send() {
    timer = 0;
    if (!document.body) return;
    let bottom = 0;
    for (const el of document.body.querySelectorAll('*')) {
      if (el.checkVisibility && !el.checkVisibility()) continue;
      bottom = Math.max(bottom, el.getBoundingClientRect().bottom + (window.scrollY || 0));
    }
    const style = getComputedStyle(document.body);
    const height = Math.ceil(bottom + (parseFloat(style.paddingBottom) || 0) + (parseFloat(style.marginBottom) || 0));
    if (height !== last) { last = height; parent.postMessage({wlframeHeight: height}, '*'); }
  }
  function schedule() { if (!timer) timer = setTimeout(send, 30); }
  if (document.body) {
    new ResizeObserver(schedule).observe(document.body);
    new MutationObserver(schedule).observe(document.body, {subtree:true, childList:true, attributes:true, characterData:true});
  }
  send();
})();
