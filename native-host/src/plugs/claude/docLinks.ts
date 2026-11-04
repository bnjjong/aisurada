// How a `#doc:<note id>` link in a model answer opens that note's preview (4.11).
// The part 3 note screen registers the opener; the answer renderer calls it.
export type DocOpener = (id: string) => boolean | void;

let opener: DocOpener | null = null;

/** Registers the opener of this window. Returns the function that removes it. */
export function registerDocOpener(next: DocOpener): () => void {
  opener = next;
  return () => {
    if (opener === next) opener = null;
  };
}

/** True when a note was found and its preview was asked to open. */
export function openDocLink(id: string): boolean {
  return opener ? opener(id) !== false : false;
}
