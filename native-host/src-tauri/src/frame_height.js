(function () {
  let last = -1, timer = 0;
  // 문서의 아래쪽 끝을 잰다. overflow가 hidden·clip·scroll·auto인 조상 안에서 잘려 보이지 않는 부분은
  // 넣지 않는다. 지도 타일처럼 상자 밖으로 삐져나온 자손이 높이를 부풀려 아래에 빈 띠를 만들거나
  // 상한(1600px)을 넘기는 것을 막기 위해서다.
  // 잘림 한계는 일반 흐름 자손(clip)과 절대 위치 자손(clipAbs)을 따로 든다. 절대 위치 자손은
  // 위치 기준 조상(containing block)이 잘림 요소 안에 있을 때만 그 요소에 잘리기 때문이다.
  function documentBottom() {
    const scrollY = window.scrollY || 0;
    let bottom = 0;
    const stack = [];
    const pushChildren = (parent, clip, clipAbs) => {
      for (let child = parent.lastElementChild; child; child = child.previousElementSibling) stack.push(child, clip, clipAbs);
    };
    pushChildren(document.body, Infinity, Infinity);
    while (stack.length) {
      const clipAbs = stack.pop(), clip = stack.pop(), el = stack.pop();
      if (el.checkVisibility && !el.checkVisibility()) continue;
      const style = getComputedStyle(el);
      const position = style.position;
      const limit = position === 'fixed' ? Infinity : position === 'absolute' ? clipAbs : clip;
      const elBottom = el.getBoundingClientRect().bottom + scrollY;
      bottom = Math.max(bottom, Math.min(elBottom, limit));
      const display = style.display;
      const clips = (style.overflowY !== 'visible' || /paint|strict|content/.test(style.contain)) && display !== 'inline' && display !== 'contents';
      const childClip = clips ? Math.min(limit, elBottom) : limit;
      const isContainingBlock = position !== 'static' || style.transform !== 'none' || style.filter !== 'none' ||
        style.perspective !== 'none' || /paint|layout|strict|content/.test(style.contain);
      pushChildren(el, childClip, isContainingBlock ? childClip : clipAbs);
    }
    return bottom;
  }
  function send() {
    timer = 0;
    if (!document.body) return;
    const bottom = documentBottom();
    const style = getComputedStyle(document.body);
    const height = Math.ceil(bottom + (parseFloat(style.paddingBottom) || 0) + (parseFloat(style.marginBottom) || 0));
    if (height !== last) { last = height; parent.postMessage({aisframeHeight: height}, '*'); }
  }
  function schedule() { if (!timer) timer = setTimeout(send, 30); }
  if (document.body) {
    new ResizeObserver(schedule).observe(document.body);
    new MutationObserver(schedule).observe(document.body, {subtree:true, childList:true, attributes:true, characterData:true});
  }
  send();
})();
