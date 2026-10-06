/**
 * NaviFS Official Website & Developer Documentation SPA Engine
 * Pure Vanilla JavaScript • Zero Dependencies • Fast & Accessible
 */

(function () {
  'use strict';

  // --- 1. Router State Management ---
  const views = {
    landing: document.getElementById('view-landing'),
    docs: document.getElementById('view-docs'),
  };

  const navLinks = document.querySelectorAll('.nav-link[data-route]');

  function navigateTo(route) {
    const isDocs = route.startsWith('#docs');

    if (isDocs) {
      views.landing.classList.remove('active');
      views.docs.classList.add('active');

      navLinks.forEach((link) => {
        link.classList.toggle('active', link.getAttribute('data-route') === '#docs');
      });

      // Handle sub-route in documentation
      const docSlug = route.replace('#docs/', '').replace('#docs', '') || 'overview';
      switchDocSection(docSlug);
      window.scrollTo(0, 0);
    } else {
      views.docs.classList.remove('active');
      views.landing.classList.add('active');

      navLinks.forEach((link) => {
        link.classList.toggle('active', link.getAttribute('data-route') === route);
      });

      if (route.startsWith('#') && route.length > 1) {
        const targetEl = document.querySelector(route);
        if (targetEl) {
          targetEl.scrollIntoView({ behavior: 'smooth' });
        }
      } else {
        window.scrollTo(0, 0);
      }
    }
  }

  window.addEventListener('hashchange', () => {
    navigateTo(window.location.hash || '#product');
  });

  // --- 2. Documentation Sub-Section Switcher ---
  const docSections = document.querySelectorAll('.doc-article');
  const sidebarLinks = document.querySelectorAll('.sidebar-nav-link');
  const breadcrumbCurrent = document.getElementById('doc-breadcrumb-current');
  const docsMobileCurrentTitle = document.getElementById('docs-mobile-current-title');

  function switchDocSection(slug) {
    let matched = false;

    docSections.forEach((section) => {
      const sectionSlug = section.getAttribute('data-doc-id');
      if (sectionSlug === slug) {
        section.style.display = 'block';
        matched = true;
        const title = section.querySelector('.doc-article-title');
        if (title) {
          if (breadcrumbCurrent) breadcrumbCurrent.textContent = title.textContent;
          if (docsMobileCurrentTitle) docsMobileCurrentTitle.textContent = title.textContent;
        }
      } else {
        section.style.display = 'none';
      }
    });

    if (!matched && docSections.length > 0) {
      docSections[0].style.display = 'block';
      if (breadcrumbCurrent) breadcrumbCurrent.textContent = 'Overview';
      if (docsMobileCurrentTitle) docsMobileCurrentTitle.textContent = 'Overview';
    }

    sidebarLinks.forEach((link) => {
      const linkSlug = link.getAttribute('data-doc-link');
      link.classList.toggle('active', linkSlug === slug);
    });
  }

  // --- Mobile Sidebar Drawer Handling ---
  const docsSidebar = document.getElementById('docs-sidebar');
  const toggleDocsBtn = document.getElementById('btn-toggle-docs-sidebar');
  const closeDocsBtn = document.getElementById('btn-close-docs-sidebar');
  const docsBackdrop = document.getElementById('docs-backdrop');

  function closeMobileSidebar() {
    if (docsSidebar) docsSidebar.classList.remove('mobile-open');
    if (docsBackdrop) docsBackdrop.classList.remove('active');
    document.body.style.overflow = '';
  }

  function openMobileSidebar() {
    if (docsSidebar) docsSidebar.classList.add('mobile-open');
    if (docsBackdrop) docsBackdrop.classList.add('active');
    document.body.style.overflow = 'hidden';
  }

  if (toggleDocsBtn) {
    toggleDocsBtn.addEventListener('click', openMobileSidebar);
  }
  if (closeDocsBtn) {
    closeDocsBtn.addEventListener('click', closeMobileSidebar);
  }
  if (docsBackdrop) {
    docsBackdrop.addEventListener('click', closeMobileSidebar);
  }

  sidebarLinks.forEach((link) => {
    link.addEventListener('click', (e) => {
      e.preventDefault();
      const slug = link.getAttribute('data-doc-link');
      closeMobileSidebar();
      window.location.hash = `#docs/${slug}`;
    });
  });

  // Documentation Sidebar Search Filter
  const docsSearchInput = document.getElementById('docs-search-input');
  if (docsSearchInput) {
    docsSearchInput.addEventListener('input', (e) => {
      const query = e.target.value.toLowerCase().trim();
      sidebarLinks.forEach((link) => {
        const text = link.textContent.toLowerCase();
        const parentItem = link.closest('.sidebar-nav-item');
        if (parentItem) {
          parentItem.style.display = text.includes(query) ? 'block' : 'none';
        }
      });
    });
  }

  // --- 3. Agent & IDE Config Switcher Tabs ---
  const tabBtns = document.querySelectorAll('.tab-btn[data-tab]');
  const tabPanes = document.querySelectorAll('.tab-pane[data-pane]');

  tabBtns.forEach((btn) => {
    btn.addEventListener('click', () => {
      const targetPane = btn.getAttribute('data-tab');

      tabBtns.forEach((b) => b.classList.toggle('active', b === btn));
      tabPanes.forEach((pane) => {
        pane.classList.toggle('active', pane.getAttribute('data-pane') === targetPane);
      });
    });
  });

  // --- 4. Interactive MCP Tool Inspector Playground ---
  const mcpTools = {
    search: {
      name: "search",
      desc: "Two-stage hybrid retrieval (FTS5 BM25 + Path + Vector similarity + Feature vector reranker) returning top-K candidates with exact evidence locators.",
      request: {
        jsonrpc: "2.0",
        id: 1,
        method: "tools/call",
        params: {
          name: "search",
          arguments: {
            query: "UAE ecommerce brands",
            filters: {
              mime_types: ["application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"],
              path_prefix: "finance/"
            },
            limit: 5
          }
        }
      },
      response: {
        jsonrpc: "2.0",
        id: 1,
        result: {
          content: [
            {
              type: "text",
              text: JSON.stringify([
                {
                  file_id: "0194a2b1-3829-7c1e-9182-3d8a92e104f7",
                  filename: "uae_ecommerce_q3.xlsx",
                  path: "finance/uae_ecommerce_q3.xlsx",
                  mime_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                  score: 0.942,
                  features: {
                    lexical_score: 0.96,
                    path_similarity: 0.88,
                    vector_score: 0.91,
                    freshness_score: 0.92,
                    extraction_quality: 1.0,
                    composite_score: 0.942
                  },
                  evidence: {
                    locator_summary: "uae_ecommerce_q3.xlsx:Worksheet 'UAE Leads' (Rows 1-50)",
                    byte_range: { start: 0, end: 4120 },
                    line_range: { start_line: 1, end_line: 50 },
                    snippet: "| Brand Name | Category | Monthly GMV (AED) |\n| Noon | Retail | 18,400,000 |\n| Namshi | Fashion | 7,200,000 |"
                  }
                }
              ], null, 2)
            }
          ]
        }
      }
    },
    open: {
      name: "open",
      desc: "Bounded content range retrieval by lines, pages, byte offsets, or chunk index with exact resource URIs and zero token explosion.",
      request: {
        jsonrpc: "2.0",
        id: 2,
        method: "tools/call",
        params: {
          name: "open",
          arguments: {
            path: "finance/uae_ecommerce_q3.xlsx",
            start_line: 1,
            end_line: 25
          }
        }
      },
      response: {
        jsonrpc: "2.0",
        id: 2,
        result: {
          content: [
            {
              type: "text",
              text: JSON.stringify({
                file_id: "0194a2b1-3829-7c1e-9182-3d8a92e104f7",
                filename: "uae_ecommerce_q3.xlsx",
                path: "finance/uae_ecommerce_q3.xlsx",
                resource_uri: "navifs://file/0194a2b1-3829-7c1e-9182-3d8a92e104f7/lines/1-25",
                content_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                locator_summary: "uae_ecommerce_q3.xlsx:Lines 1-25",
                byte_range: { start: 0, end: 2048 },
                line_range: { start_line: 1, end_line: 25 },
                page_range: null,
                content: "| Brand Name | Category | Monthly GMV (AED) |\n| Noon | Retail | 18,400,000 |\n| Careem Quik | Grocery | 5,100,000 |",
                total_lines: 25,
                truncated: false,
                chunks_included: ["0194a2b1-3829-7000-0000-000000000001"]
              }, null, 2)
            }
          ]
        }
      }
    },
    inspect: {
      name: "inspect",
      desc: "Structural metadata, MIME classification, chunk statistics, worksheet/heading outline entities, and recent temporal lifecycle events.",
      request: {
        jsonrpc: "2.0",
        id: 3,
        method: "tools/call",
        params: {
          name: "inspect",
          arguments: {
            path: "finance/uae_ecommerce_q3.xlsx"
          }
        }
      },
      response: {
        jsonrpc: "2.0",
        id: 3,
        result: {
          content: [
            {
              type: "text",
              text: JSON.stringify({
                file_id: "0194a2b1-3829-7c1e-9182-3d8a92e104f7",
                filename: "uae_ecommerce_q3.xlsx",
                path: "finance/uae_ecommerce_q3.xlsx",
                mime_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                size_bytes: 48920,
                content_hash: "a4f89d31b2c45e89d123456789abcdef0123456789abcdef0123456789abcdef",
                status: "Indexed",
                created_at: "2026-09-15T08:30:00Z",
                modified_at: "2026-09-28T14:22:10Z",
                indexed_at: "2026-10-04T12:00:00Z",
                chunk_count: 6,
                structure: {
                  kind: "spreadsheet",
                  sheets: ["UAE Leads", "Contacts", "Revenue_Breakdown"],
                  sections: null
                },
                recent_events: [
                  { id: "evt-01", event_type: "indexed", timestamp: "2026-10-04T12:00:00Z", source: "pipeline", metadata: { chunks_count: 6 } },
                  { id: "evt-02", event_type: "modified", timestamp: "2026-09-28T14:22:10Z", source: "watcher", metadata: { size_bytes: 48920 } }
                ]
              }, null, 2)
            }
          ]
        }
      }
    },
    related: {
      name: "related",
      desc: "One-to-one hop knowledge graph traversal discovering adjacent nodes, directional relation edges (Contains, Imports, References), and edge weights.",
      request: {
        jsonrpc: "2.0",
        id: 4,
        method: "tools/call",
        params: {
          name: "related",
          arguments: {
            path: "finance/uae_ecommerce_q3.xlsx",
            relation_type: "Contains"
          }
        }
      },
      response: {
        jsonrpc: "2.0",
        id: 4,
        result: {
          content: [
            {
              type: "text",
              text: JSON.stringify({
                center_entity: {
                  id: "0194a2b1-3829-7c1e-9182-3d8a92e104f7",
                  name: "uae_ecommerce_q3.xlsx",
                  entity_type: "Document",
                  file_id: "0194a2b1-3829-7c1e-9182-3d8a92e104f7",
                  properties: {}
                },
                outgoing: [
                  {
                    edge_id: "edge-01",
                    relation_type: "Contains",
                    weight: 1.0,
                    entity_id: "entity-sheet-01",
                    entity: {
                      id: "entity-sheet-01",
                      name: "UAE Leads",
                      entity_type: "Heading",
                      properties: { "sheet": "UAE Leads", "type": "worksheet" }
                    }
                  }
                ],
                incoming: [],
                total_connections: 1
              }, null, 2)
            }
          ]
        }
      }
    }
  };

  const toolItems = document.querySelectorAll('.mcp-tool-item[data-tool]');
  const mcpTitle = document.getElementById('mcp-active-tool-title');
  const mcpDesc = document.getElementById('mcp-active-tool-desc');
  const mcpReqCode = document.getElementById('mcp-req-code');
  const mcpResCode = document.getElementById('mcp-res-code');

  function renderMcpPlayground(toolKey) {
    const data = mcpTools[toolKey];
    if (!data) return;

    if (mcpTitle) mcpTitle.textContent = `tools/call: ${data.name}`;
    if (mcpDesc) mcpDesc.textContent = data.desc;
    if (mcpReqCode) mcpReqCode.textContent = JSON.stringify(data.request, null, 2);
    if (mcpResCode) mcpResCode.textContent = JSON.stringify(data.response, null, 2);

    toolItems.forEach((item) => {
      item.classList.toggle('active', item.getAttribute('data-tool') === toolKey);
    });
  }

  toolItems.forEach((item) => {
    item.addEventListener('click', () => {
      const toolKey = item.getAttribute('data-tool');
      renderMcpPlayground(toolKey);
    });
  });

  // --- 5. Clipboard Copy Utility with Visual Feedback ---
  const installCmds = {
    windows: 'powershell -ExecutionPolicy Bypass -Command "irm https://raw.githubusercontent.com/Dhruv6190/NaviFS/main/install.ps1 | iex"',
    unix: 'curl -fsSL https://raw.githubusercontent.com/Dhruv6190/NaviFS/main/install.sh | bash',
    cli: 'cargo install --git https://github.com/Dhruv6190/NaviFS.git navifs-daemon',
  };

  const segmentBtns = document.querySelectorAll('.install-segment-btn[data-platform]');
  const cmdInstallEl = document.getElementById('cmd-install');

  segmentBtns.forEach((btn) => {
    btn.addEventListener('click', () => {
      const platform = btn.getAttribute('data-platform');
      segmentBtns.forEach((b) => b.classList.toggle('active', b === btn));
      if (cmdInstallEl && installCmds[platform]) {
        cmdInstallEl.textContent = installCmds[platform];
      }
    });
  });

  window.copyCurrentInstallSnippet = function (btnElement) {
    if (!cmdInstallEl) return;
    const text = cmdInstallEl.textContent || cmdInstallEl.innerText;
    window.copySnippetText(text, btnElement);
  };

  window.copySnippetText = function (text, btnElement) {
    if (!text) return;
    navigator.clipboard.writeText(text).then(() => {
      const originalHtml = btnElement.innerHTML;
      btnElement.innerHTML = `
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="#2997ff" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
          <polyline points="20 6 9 17 4 12"></polyline>
        </svg> <span>Copied!</span>
      `;
      setTimeout(() => {
        btnElement.innerHTML = originalHtml;
      }, 2000);
    });
  };

  window.copySnippet = function (targetId, btnElement) {
    const el = document.getElementById(targetId);
    if (!el) return;

    const text = el.innerText || el.textContent;
    window.copySnippetText(text, btnElement);
  };

  // --- 6. Initial Page Boot ---
  window.addEventListener('DOMContentLoaded', () => {
    navigateTo(window.location.hash || '#product');
    renderMcpPlayground('search');
  });

})();
