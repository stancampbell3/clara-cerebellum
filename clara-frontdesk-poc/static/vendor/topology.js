// Live Ritual/evaluator topology view — a Cytoscape.js graph fed by the
// "topology" WS frames pushed every ~5s (see src/ws.rs's poll_topology).
// Vanilla JS, no build step, matching this page's existing conventions
// (cytoscape.min.js vendored the same way as marked/highlight/purify/viz).
//
// Ports Cobbler's ICON_ALIASES / fallback-on-404 technique (dagda/cobbler/
// frontend/src/components/GraphCanvas/iconUtils.ts) so evaluator nodes reuse
// the same lild_*.svg artwork, served here from vendor/icons/ instead of an
// API's /assets route.

(function () {
  const ICON_ALIASES = {
    clara_mind_splinter_groq: 'groq',
    clara_mind_splinter: 'clara',
    // No dedicated artwork for these yet — fall back to the default daemon icon.
    snek: 'default',
    ollama_local: 'default',
  };

  function iconUrlFor(evaluatorName) {
    const alias = ICON_ALIASES[evaluatorName] || evaluatorName || 'default';
    return `vendor/icons/lild_${alias}.svg`;
  }

  // Probe once whether an evaluator-specific SVG actually exists; falls
  // back to lild_default.svg on a 404 rather than showing a broken image.
  function resolveIconUrl(evaluatorName, onResolved) {
    const url = iconUrlFor(evaluatorName);
    const img = new Image();
    img.onload = () => onResolved(url);
    img.onerror = () => onResolved('vendor/icons/lild_default.svg');
    img.src = url;
  }

  const STYLESHEET = [
    {
      selector: 'node',
      style: {
        label: 'data(label)',
        'font-size': 10,
        'font-family': 'var(--font-body, Georgia, serif)',
        color: '#d4c8b8',
        'text-wrap': 'wrap',
        'text-max-width': '90px',
        'text-valign': 'bottom',
        'text-margin-y': 6,
      },
    },
    {
      selector: 'node[kind = "ritual"]',
      style: {
        shape: 'round-rectangle',
        width: 90,
        height: 40,
        'background-color': 'rgba(22, 10, 10, 0.93)',
        'border-width': 2,
        'border-color': '#7b1a1a',
        'text-valign': 'center',
        'text-margin-y': 0,
        'font-weight': 'bold',
      },
    },
    {
      selector: 'node[kind = "evaluator"]',
      style: {
        shape: 'round-rectangle',
        width: 56,
        height: 56,
        'background-color': '#1a1010',
        'background-image': 'data(iconUrl)',
        'background-fit': 'contain',
        'background-width': '70%',
        'background-height': '70%',
        'border-width': 3,
        'border-color': '#3d1a1a',
      },
    },
    // Evaluator liveness state → border color (mirrors the Grafana
    // Evaluator Liveness dashboard's state vocabulary).
    { selector: 'node[kind = "evaluator"][state = "idle"]', style: { 'border-color': '#2e7d32' } },
    { selector: 'node[kind = "evaluator"][state = "busy"]', style: { 'border-color': '#1565c0' } },
    { selector: 'node[kind = "evaluator"][state = "starting"]', style: { 'border-color': '#f9a825' } },
    { selector: 'node[kind = "evaluator"][state = "error"]', style: { 'border-color': '#c0392b' } },
    { selector: 'node[kind = "evaluator"][state = "stopped"]', style: { 'border-color': '#5a5a5a' } },
    { selector: 'node[kind = "ritual"][state = "terminated"]', style: { 'border-color': '#5a5a5a' } },
    {
      selector: 'edge',
      style: {
        width: 1.5,
        'line-color': '#8a8a8a',
        'curve-style': 'bezier',
        'target-arrow-shape': 'none',
      },
    },
  ];

  class TopologyView {
    constructor(container) {
      this.cy = cytoscape({
        container,
        style: STYLESHEET,
        layout: { name: 'cose', animate: false },
        elements: [],
        wheelSensitivity: 0.3,
      });
      this._pendingIcons = new Set();
    }

    /// `nodes`: [{id, kind, label, state, icon_alias?}], `edges`: [{source, target}]
    update(nodes, edges) {
      const cy = this.cy;
      const desiredNodeIds = new Set(nodes.map((n) => n.id));
      const desiredEdgeIds = new Set(edges.map((e) => `${e.source}->${e.target}`));

      let structuralChange = false;

      cy.nodes().forEach((el) => {
        if (!desiredNodeIds.has(el.id())) {
          el.remove();
          structuralChange = true;
        }
      });
      cy.edges().forEach((el) => {
        const id = `${el.data('source')}->${el.data('target')}`;
        if (!desiredEdgeIds.has(id)) {
          el.remove();
          structuralChange = true;
        }
      });

      for (const n of nodes) {
        const existing = cy.getElementById(n.id);
        if (existing.length) {
          existing.data({ label: n.label, state: n.state });
        } else {
          structuralChange = true;
          const data = { id: n.id, kind: n.kind, label: n.label, state: n.state };
          if (n.kind === 'evaluator') {
            data.iconUrl = iconUrlFor(n.icon_alias);
          }
          cy.add({ group: 'nodes', data });
          if (n.kind === 'evaluator' && !this._pendingIcons.has(n.id)) {
            this._pendingIcons.add(n.id);
            resolveIconUrl(n.icon_alias, (url) => {
              this._pendingIcons.delete(n.id);
              const node = cy.getElementById(n.id);
              if (node.length) node.data('iconUrl', url);
            });
          }
        }
      }

      for (const e of edges) {
        const id = `${e.source}->${e.target}`;
        if (cy.getElementById(id).length === 0) {
          structuralChange = true;
          cy.add({ group: 'edges', data: { id, source: e.source, target: e.target } });
        }
      }

      // Only re-layout on an actual structural change (nodes/edges added or
      // removed) — a plain state-color update every 5s should not jitter
      // node positions.
      if (structuralChange) {
        cy.layout({ name: 'cose', animate: false }).run();
      }
    }
  }

  window.ClaraTopology = { TopologyView };
})();
