/**
 * Actions owned by mounted components (overlays, panes) that commands and shortcuts can trigger.
 * Components replace these no-ops while they're mounted.
 */
export const uiActions = {
  /** Open the command bar with this text (">" for commands, "" to go to a file or folder…). */
  openCommandBar: (_text: string = ">") => {},
  openSettings: () => {},
  openAbout: () => {},
  /** Type a path in the address bar (Ctrl+L). */
  focusAddress: () => {},
  /** Put the keyboard back in the folder view. */
  focusList: () => {},
};
