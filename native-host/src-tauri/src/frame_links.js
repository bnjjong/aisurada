document.addEventListener('click', function (event) {
  if (event.defaultPrevented || event.button !== 0) return;
  const anchor = event.target.closest && event.target.closest('a');
  if (!anchor) return;
  const url = (anchor.getAttribute('href') || anchor.getAttribute('xlink:href') || '').trim();
  if (/^(https?:|mailto:|tel:)/i.test(url)) {
    event.preventDefault();
    parent.postMessage({wlframeOpen: url}, '*');
  }
});
