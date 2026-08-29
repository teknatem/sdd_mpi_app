// Редактор кода страницы разработки плагина (CodeMirror 6).
//
// Тема НЕ зашита в бандл: и оформление редактора, и палитра подсветки заданы
// через `var(--...)` — те же токены, которыми красится остальное приложение.
// Поэтому переключение темы перекрашивает открытый редактор само, без
// пересоздания EditorView: CSS-переменные наследуются от контейнера
// `.plugin-code-editor`, а его красит `static/pages/plugins.css`.
//
// Сборка: `pnpm build:plugin-editor` → `crates/frontend/static/plugin-editor.js`.

import { basicSetup, EditorView } from "codemirror";
import { EditorState } from "@codemirror/state";
import { keymap } from "@codemirror/view";
import { indentWithTab } from "@codemirror/commands";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";
import { css } from "@codemirror/lang-css";
import { javascript } from "@codemirror/lang-javascript";
import { SQLite, sql } from "@codemirror/lang-sql";

function languageExtension(language) {
  switch (language) {
    case "css":
      return css();
    case "sql":
      return sql({ dialect: SQLite });
    default:
      return javascript({ jsx: false, typescript: false });
  }
}

// Хром редактора: фон отдаём контейнеру (`--color-code-bg`), сами красим
// только то, что CodeMirror рисует поверх него.
const appTheme = EditorView.theme({
  "&": {
    color: "var(--color-text-primary)",
    backgroundColor: "transparent",
  },
  ".cm-content": {
    caretColor: "var(--color-text-primary)",
    fontFamily: "var(--font-family-mono)",
  },
  ".cm-cursor, .cm-dropCursor": {
    borderLeftColor: "var(--color-text-primary)",
  },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection":
    { backgroundColor: "var(--color-active)" },
  ".cm-selectionMatch": { backgroundColor: "var(--color-active)" },
  ".cm-searchMatch": {
    backgroundColor: "var(--color-active)",
    outline: "1px solid var(--color-border)",
  },
  ".cm-searchMatch.cm-searchMatch-selected": {
    backgroundColor: "var(--table-row-selected)",
  },
  ".cm-activeLine": { backgroundColor: "var(--color-hover)" },
  ".cm-gutters": {
    backgroundColor: "transparent",
    color: "var(--color-text-tertiary)",
    border: "none",
    borderRight: "1px solid var(--color-border-light)",
  },
  ".cm-activeLineGutter": {
    backgroundColor: "var(--color-hover)",
    color: "var(--color-text-secondary)",
  },
  ".cm-foldPlaceholder": {
    backgroundColor: "var(--color-active)",
    border: "none",
    color: "var(--color-text-secondary)",
  },
  ".cm-matchingBracket, &.cm-focused .cm-matchingBracket": {
    backgroundColor: "var(--color-active)",
    outline: "1px solid var(--color-border)",
  },
  ".cm-nonmatchingBracket, &.cm-focused .cm-nonmatchingBracket": {
    color: "var(--color-error)",
  },
  ".cm-panels": {
    backgroundColor: "var(--color-surface)",
    color: "var(--color-text-primary)",
    borderColor: "var(--color-border)",
  },
  ".cm-panels input, .cm-panels button, .cm-panels select": {
    backgroundColor: "var(--form-input-bg)",
    color: "var(--color-text-primary)",
    border: "1px solid var(--form-input-border)",
    borderRadius: "var(--radius-sm)",
  },
  ".cm-tooltip": {
    backgroundColor: "var(--color-menu-surface)",
    color: "var(--color-text-primary)",
    border: "1px solid var(--color-border)",
  },
  ".cm-tooltip-autocomplete > ul > li[aria-selected]": {
    backgroundColor: "var(--color-active)",
    color: "var(--color-text-primary)",
  },
});

// Палитра подсветки — только семантические токены приложения: они определены
// в каждой теме, значит подсветка меняется вместе с темой без веток в коде.
const appHighlight = HighlightStyle.define([
  {
    tag: [t.comment, t.lineComment, t.blockComment, t.docComment],
    // Курсив уже отделяет комментарий от кода, поэтому берём secondary, а не
    // tertiary: на светлой теме tertiary (#9ca3af) на фоне #f3f4f6 почти не читается.
    color: "var(--color-text-secondary)",
    fontStyle: "italic",
  },
  {
    tag: [t.keyword, t.controlKeyword, t.moduleKeyword, t.operatorKeyword],
    color: "var(--color-primary)",
    fontWeight: "600",
  },
  {
    tag: [t.string, t.special(t.string), t.regexp],
    color: "var(--color-success)",
  },
  { tag: [t.number, t.bool, t.null, t.atom], color: "var(--color-warning)" },
  {
    tag: [t.function(t.variableName), t.function(t.propertyName), t.labelName],
    color: "var(--color-accent)",
  },
  {
    tag: [t.typeName, t.className, t.namespace, t.tagName],
    color: "var(--color-accent)",
  },
  {
    tag: [t.propertyName, t.attributeName, t.definition(t.variableName)],
    color: "var(--color-link)",
  },
  {
    tag: [t.operator, t.punctuation, t.separator, t.bracket, t.meta],
    color: "var(--color-text-tertiary)",
  },
  { tag: t.link, color: "var(--color-link)", textDecoration: "underline" },
  { tag: t.strong, fontWeight: "bold" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: t.invalid, color: "var(--color-error)" },
]);

window.PluginCodeEditor = Object.freeze({
  /// `onSave` — реакция на Ctrl/Cmd+S внутри редактора (кнопка «Сохранить»
  /// страницы). Без него сочетание уходит браузеру и сохраняет HTML-страницу.
  create(parent, language, value, onChange, onSave) {
    const saveKeymap = onSave
      ? [
          {
            key: "Mod-s",
            preventDefault: true,
            run: () => {
              onSave();
              return true;
            },
          },
        ]
      : [];

    return new EditorView({
      parent,
      doc: value || "",
      extensions: [
        basicSetup,
        languageExtension(language),
        appTheme,
        syntaxHighlighting(appHighlight),
        EditorState.tabSize.of(2),
        // Tab внутри редактора ставит отступ, а не уводит фокус с поля.
        keymap.of([...saveKeymap, indentWithTab]),
        EditorView.lineWrapping,
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            onChange(update.state.doc.toString());
          }
        }),
      ],
    });
  },

  setValue(editor, value) {
    const next = value || "";
    const current = editor.state.doc.toString();
    if (current === next) return;
    editor.dispatch({
      changes: { from: 0, to: current.length, insert: next },
    });
  },

  destroy(editor) {
    editor.destroy();
  },
});
