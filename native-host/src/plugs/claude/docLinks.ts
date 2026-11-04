// How a `#doc:<note id>` link in a model answer opens that note's preview (4.11).
// The part 3 note screen registers the opener; the answer renderer calls it.
export type DocOpener = (id: string) => boolean | void;

const openers: Array<{ open: DocOpener }> = [];

/** Registers the opener of this window. Returns the function that removes it. */
export function registerDocOpener(next: DocOpener): () => void {
  const entry = { open: next };
  openers.push(entry);
  return () => {
    const index = openers.indexOf(entry);
    if (index >= 0) openers.splice(index, 1);
  };
}

/** True when a note was found and its preview was asked to open. */
export function openDocLink(id: string): boolean {
  return openers.length ? openers[openers.length - 1].open(id) !== false : false;
}
