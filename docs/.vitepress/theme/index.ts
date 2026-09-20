// VitePress theme extension.
//
// Two things are rendered in the browser rather than at build time, because both
// are heavy and neither is needed until a page actually uses one:
//
//   * `scratchblocks` — turns `<pre class="blocks">` into Scratch's own block
//     pictures. Its build evaluates `window` at module scope and appends its
//     stylesheets on import, so it can only be loaded in a browser; hence the
//     dynamic import inside the render call rather than a top-level import.
//   * `mermaid` — turns `<pre class="mermaid">` (a ```mermaid fence) into a
//     diagram. It is imported lazily for the same reason.
//
// Both renderers replace the *contents* of the element they are given, so a
// second call would try to parse their own output. Rendered elements are
// therefore marked, and the selectors skip them.

import DefaultTheme from "vitepress/theme";
import { nextTick, onMounted, watch } from "vue";
import { useData, useRoute } from "vitepress";
import "./custom.css";

const SCRATCH_SELECTOR = "pre.blocks:not([data-sb]), code.b:not([data-sb])";

export default {
  extends: DefaultTheme,
  setup() {
    const route = useRoute();
    const { isDark } = useData();

    const renderScratchblocks = async () => {
      if (typeof window === "undefined") return;

      const pending = Array.from(document.querySelectorAll(SCRATCH_SELECTOR));
      if (pending.length === 0) return;

      const module = await import("scratchblocks");
      const scratchblocks = (module as { default?: unknown }).default ?? module;
      const render = (scratchblocks as { renderMatching?: (s: string, o?: unknown) => void })
        .renderMatching;
      if (typeof render !== "function") {
        console.error("scratchblocks: renderMatching is unavailable");
        return;
      }

      render("pre.blocks:not([data-sb])", {
        style: "scratch3",
        languages: ["en"],
        scale: 0.9,
      });
      render("code.b:not([data-sb])", {
        inline: true,
        style: "scratch3",
        languages: ["en"],
        scale: 0.9,
      });

      // Mark what was just rendered so navigating back does not reparse it.
      for (const el of pending) el.setAttribute("data-sb", "1");
    };

    // `redraw` re-renders diagrams that were drawn for the other colour scheme.
    // Mermaid bakes its palette into the SVG, so a theme change means drawing
    // them again — from the source text kept in `data-src`.
    const renderMermaid = async (redraw = false) => {
      if (typeof window === "undefined") return;

      const all = Array.from(document.querySelectorAll<HTMLElement>("pre.mermaid"));
      if (all.length === 0) return;

      if (redraw) {
        for (const el of all) {
          const source = el.dataset.src;
          if (source === undefined) continue;
          el.textContent = source;
          el.removeAttribute("data-mermaid");
        }
      }

      const pending = all.filter((el) => !el.hasAttribute("data-mermaid"));
      if (pending.length === 0) return;

      const mermaid = (await import("mermaid")).default;
      mermaid.initialize({
        startOnLoad: false,
        securityLevel: "loose",
        theme: document.documentElement.classList.contains("dark") ? "dark" : "default",
        fontFamily: "inherit",
      });

      for (const el of pending) {
        if (el.dataset.src === undefined) el.dataset.src = el.textContent ?? "";
      }

      try {
        await mermaid.run({ nodes: pending });
        for (const el of pending) el.setAttribute("data-mermaid", "1");
      } catch (error) {
        console.error("mermaid:", error);
      }
    };

    const renderAll = async (redraw = false) => {
      await renderScratchblocks();
      await renderMermaid(redraw);
    };

    onMounted(() => {
      void renderAll();
    });

    watch(
      () => route.path,
      () => nextTick(() => void renderAll())
    );

    watch(isDark, () => void renderMermaid(true));
  },
};
