import { defineConfig } from "vitepress";
import rasmGrammar from "./rasm.tmLanguage.json";
import ravGrammar from "./rav.tmLanguage.json";

// One language, one target: raven compiles to raven-asm, raven-asm compiles to a
// Scratch 3 project. `base` is the repository name, so the published root is
// https://raven-scratch.github.io/raven/.

export default defineConfig({
  base: "/raven/",
  cleanUrls: true,
  head: [["link", { rel: "icon", type: "image/svg+xml", href: "/raven/logo.svg" }]],

  title: "raven",
  description:
    "Two languages for Scratch 3: raven-asm writes one block per statement, raven adds types, macros and sugar on top of it.",

  markdown: {
    // Highlight ```rasm and ```rav fences with the grammars that ship with the
    // docs; both are plain TextMate grammars and work in any editor too.
    languages: [rasmGrammar as never, ravGrammar as never],

    // A ```mermaid fence is emitted as a `<pre class="mermaid">`, which the
    // theme renders in the browser. Mermaid is a large dependency, so it is
    // loaded lazily and never on the server.
    config(md) {
      const defaultFence = md.renderer.rules.fence;
      md.renderer.rules.fence = (tokens, index, options, env, self) => {
        const token = tokens[index];
        if (token.info.trim() === "mermaid") {
          return `<pre class="mermaid">${md.utils.escapeHtml(token.content)}</pre>\n`;
        }
        return defaultFence
          ? defaultFence(tokens, index, options, env, self)
          : self.renderToken(tokens, index, options);
      };
    },
  },

  sitemap: { hostname: "https://raven-scratch.github.io/raven/" },

  themeConfig: {
    logo: "/logo.svg",

    nav: [
      { text: "Guide", link: "/guide/", activeMatch: "^/guide/" },
      { text: "raven-asm", link: "/raven-asm/", activeMatch: "^/raven-asm/" },
      { text: "raven", link: "/raven/", activeMatch: "^/raven/" },
      { text: "Reference", link: "/reference/blocks", activeMatch: "^/reference/" },
    ],

    sidebar: {
      "/guide/": [
        {
          text: "Introduction",
          items: [
            { text: "raven and raven-asm", link: "/guide/" },
            { text: "Getting started", link: "/guide/getting-started" },
            { text: "For LLMs", link: "/guide/for-llms" },
          ],
        },
        {
          text: "The other language",
          items: [
            { text: "What is raven?", link: "/raven/" },
            { text: "Design laws", link: "/raven/design" },
            { text: "From raven to Scratch", link: "/raven/lowering" },
          ],
        },
      ],

      "/raven-asm/": [
        {
          text: "Introduction",
          items: [
            { text: "What is raven-asm?", link: "/raven-asm/" },
            { text: "Project structure", link: "/raven-asm/project-structure" },
          ],
        },
        {
          text: "Language",
          items: [
            { text: "Syntax", link: "/raven-asm/syntax" },
            { text: "Blocks and Scratch", link: "/raven-asm/blocks" },
            { text: "Variables, lists and broadcasts", link: "/raven-asm/variables" },
            { text: "Procedures", link: "/raven-asm/procedures" },
            { text: "Assets", link: "/raven-asm/assets" },
            { text: "Multiple files", link: "/raven-asm/multi-file" },
          ],
        },
        {
          text: "Using the tool",
          items: [
            { text: "Command line", link: "/raven-asm/cli" },
            { text: "Troubleshooting", link: "/raven-asm/troubleshooting" },
          ],
        },
        {
          text: "Design",
          items: [{ text: "Why no syntax sugar", link: "/raven-asm/design" }],
        },
      ],

      "/raven/": [
        {
          text: "Introduction",
          items: [
            { text: "What is raven?", link: "/raven/" },
            { text: "Design laws", link: "/raven/design" },
          ],
        },
        {
          text: "Language",
          items: [
            { text: "Syntax", link: "/raven/syntax" },
            { text: "Types and shapes", link: "/raven/types" },
            { text: "Macros", link: "/raven/macros" },
            { text: "Modules", link: "/raven/modules" },
            { text: "Standard library", link: "/raven/std" },
          ],
        },
        {
          text: "Implementation",
          items: [
            { text: "From raven to Scratch", link: "/raven/lowering" },
            { text: "Command line", link: "/raven/cli" },
            { text: "Coming from Scrust", link: "/raven/from-scrust" },
          ],
        },
      ],

      "/reference/": [
        {
          text: "Reference",
          items: [{ text: "Block reference", link: "/reference/blocks" }],
        },
      ],
    },

    socialLinks: [{ icon: "github", link: "https://github.com/raven-scratch/raven" }],

    search: { provider: "local" },

    outline: { level: [2, 3] },

    footer: {
      message: "Released under the MIT License.",
      copyright: "Copyright © 2025 DilemmaGX",
    },
  },
});
