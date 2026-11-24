(() => {
  const palettes = {
    light: {
      bg: "#fcfdfe",
      fg: "#1a1d23",
      muted: "#575b63",
      surface: "#f4f6f8",
      strong: "#ebedf1",
      hairline: "#c1c4c9",
      accent: "#00686a",
      soft: "#e2f5f4",
      second: "#b8740f",
    },
    dark: {
      bg: "#101215",
      fg: "#e8e9ed",
      muted: "#a1a5ab",
      surface: "#18191d",
      strong: "#1f2226",
      hairline: "#45484d",
      accent: "#5ed0c7",
      soft: "#162e2e",
      second: "#e2a64a",
    },
  };
  const darkThemes = ["coal", "navy", "ayu"];
  const root = document.documentElement;

  function scheme() {
    return darkThemes.some((name) => root.classList.contains(name)) ? "dark" : "light";
  }

  function variables(colors) {
    return {
      darkMode: colors === palettes.dark,
      background: colors.bg,
      fontFamily: getComputedStyle(root).getPropertyValue("--ui-font") || "system-ui, sans-serif",
      fontSize: "14px",
      textColor: colors.fg,
      primaryColor: colors.surface,
      primaryTextColor: colors.fg,
      primaryBorderColor: colors.hairline,
      secondaryColor: colors.soft,
      secondaryTextColor: colors.fg,
      secondaryBorderColor: colors.accent,
      tertiaryColor: colors.strong,
      tertiaryTextColor: colors.fg,
      tertiaryBorderColor: colors.hairline,
      lineColor: colors.muted,
      mainBkg: colors.surface,
      nodeBorder: colors.hairline,
      clusterBkg: colors.bg,
      clusterBorder: colors.hairline,
      titleColor: colors.fg,
      edgeLabelBackground: colors.bg,
      actorBkg: colors.surface,
      actorBorder: colors.hairline,
      actorTextColor: colors.fg,
      actorLineColor: colors.hairline,
      signalColor: colors.fg,
      signalTextColor: colors.fg,
      labelBoxBkgColor: colors.strong,
      labelBoxBorderColor: colors.hairline,
      labelTextColor: colors.fg,
      loopTextColor: colors.muted,
      noteBkgColor: colors.soft,
      noteTextColor: colors.fg,
      noteBorderColor: colors.accent,
      activationBkgColor: colors.soft,
      activationBorderColor: colors.accent,
      sequenceNumberColor: colors.bg,
      altSectionBkgColor: colors.surface,
      xyChart: {
        backgroundColor: colors.bg,
        titleColor: colors.fg,
        xAxisLabelColor: colors.muted,
        xAxisTitleColor: colors.muted,
        xAxisTickColor: colors.hairline,
        xAxisLineColor: colors.hairline,
        yAxisLabelColor: colors.muted,
        yAxisTitleColor: colors.muted,
        yAxisTickColor: colors.hairline,
        yAxisLineColor: colors.hairline,
        plotColorPalette: `${colors.accent}, ${colors.second}`,
      },
    };
  }

  const blocks = [...document.querySelectorAll("pre.mermaid")];
  for (const block of blocks) block.dataset.source = block.textContent;

  let drawnScheme = null;

  async function draw() {
    if (typeof mermaid === "undefined" || blocks.length === 0) return;
    const current = scheme();
    if (current === drawnScheme) return;
    drawnScheme = current;
    mermaid.initialize({
      startOnLoad: false,
      securityLevel: "strict",
      theme: "base",
      themeVariables: variables(palettes[current]),
      flowchart: { curve: "basis", padding: 14 },
      sequence: { mirrorActors: false, messageAlign: "center" },
    });
    for (const block of blocks) {
      block.removeAttribute("data-processed");
      block.removeAttribute("data-drawn");
      block.textContent = block.dataset.source;
    }
    try {
      await mermaid.run({ nodes: blocks });
    } finally {
      for (const block of blocks) block.dataset.drawn = "";
    }
  }

  const darkLabel = document.getElementById("mdbook-theme-coal");
  if (darkLabel) darkLabel.textContent = "Dark";

  new MutationObserver(() => void draw()).observe(root, { attributes: true, attributeFilter: ["class"] });
  void draw();
})();
