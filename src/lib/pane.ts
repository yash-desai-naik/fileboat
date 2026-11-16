/**
 * What the active folder can do, for commands and shortcuts: the active tab's pane fills these in
 * (and empties them when another tab takes over).
 */
export const paneActions = {
  newFolder: () => {},
  newFile: () => {},
  rename: () => {},
  refresh: () => {},
  /** Delete the selection: to the Recycle Bin / Trash, or permanently (asks first). */
  delete: (_permanently: boolean) => {},
};

export type PaneActions = typeof paneActions;
