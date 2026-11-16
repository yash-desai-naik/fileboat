/**
 * What the active folder can do, for commands and shortcuts: the active tab's pane fills these in
 * (and empties them when another tab takes over).
 */
export const paneActions = {
  newFolder: () => {},
  newFile: () => {},
  rename: () => {},
  refresh: () => {},
};

export type PaneActions = typeof paneActions;
