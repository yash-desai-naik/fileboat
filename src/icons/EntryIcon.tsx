import { memo } from "react";
import { useSettings } from "../lib/settings";
import { usePrefersDark, shownMode } from "../lib/themeMode";
import { FileGlyph, FolderGlyph, fileLook, folderLook, FOLDER_PLAIN, type FileLook, type FolderLook, type FolderMark, type Mark } from "./builtin";
import { customImage, ruleFor, systemIcon, themeIconUrl, useIconsVersion, vsIconId, vsTheme } from "./icons";

interface Props {
  name: string;
  /** Lower-case extension ("" for folders and dotfiles). */
  ext: string;
  folder: boolean;
  /** The full path, when known (for "just this folder" rules and system icons of programs). */
  path?: string | null;
  /** An open folder (the folder you're in, or one open in Columns). */
  open?: boolean;
  size?: number;
}

function Img({ src, size }: { src: string; size: number }) {
  return <img className="icon-img" src={src} width={size} height={size} alt="" draggable={false} decoding="async" />;
}

/** A file or folder's icon in the chosen icon theme, after the user's own rules. */
export const EntryIcon = memo(function EntryIcon({ name, ext, folder, path = null, open = false, size = 15 }: Props) {
  const settings = useSettings();
  useIconsVersion();
  const dark = usePrefersDark();
  const light = shownMode(settings.theme, dark) === "light";
  const rule = ruleFor(path, name, ext, settings.iconAssociations);
  const themeId = settings.iconTheme;

  // A picture chosen for it.
  if (rule?.icon?.startsWith("file:")) {
    const url = customImage(rule.icon.slice(5));
    if (url) return <Img src={url} size={size} />;
  }

  if (themeId.startsWith("vscode:")) {
    const t = vsTheme(themeId.slice(7));
    if (t) {
      const id = rule?.icon && !rule.icon.startsWith("file:") && t.defs[rule.icon] ? rule.icon : vsIconId(t, name, folder, open, light);
      const url = themeIconUrl(t, id);
      if (url) return <Img src={url} size={size} />;
    }
  } else if (themeId === "system" && path) {
    const url = systemIcon(path, ext, folder);
    if (url) return <Img src={url} size={size} />;
  }

  // Built in (also while a theme or system icon loads).
  const minimal = themeId === "minimal" || themeId === "system";
  if (folder) {
    const base: FolderLook = minimal ? FOLDER_PLAIN : folderLook(name);
    const mark = rule?.icon && !rule.icon.startsWith("file:") ? (rule.icon as FolderMark) : base.mark;
    return <FolderGlyph look={{ color: rule?.color ?? base.color, mark }} size={size} open={open} />;
  }
  const base: FileLook = minimal ? { color: "var(--text-3)", mark: "none" } : fileLook(name, ext);
  const mark = rule?.icon && !rule.icon.startsWith("file:") ? (rule.icon as Mark) : base.mark;
  return <FileGlyph look={{ color: rule?.color ?? base.color, mark }} size={size} />;
});
