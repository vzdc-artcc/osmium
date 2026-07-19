use pulldown_cmark::{Options, Parser, html};

pub struct DocPage {
    pub title: &'static str,
    pub section: &'static str,
    pub slug: &'static str,
    pub markdown: &'static str,
}

pub static DOC_PAGES: &[DocPage] = &[
    DocPage {
        title: "Docs Home",
        section: "",
        slug: "index",
        markdown: include_str!("../../docs/index.md"),
    },
    DocPage {
        title: "Local Development",
        section: "getting-started",
        slug: "local-development",
        markdown: include_str!("../../docs/getting-started/local-development.md"),
    },
    DocPage {
        title: "Configuration",
        section: "getting-started",
        slug: "configuration",
        markdown: include_str!("../../docs/getting-started/configuration.md"),
    },
    DocPage {
        title: "Migrations",
        section: "getting-started",
        slug: "migrations",
        markdown: include_str!("../../docs/getting-started/migrations.md"),
    },
    DocPage {
        title: "Testing",
        section: "getting-started",
        slug: "testing",
        markdown: include_str!("../../docs/getting-started/testing.md"),
    },
    DocPage {
        title: "Architecture Overview",
        section: "architecture",
        slug: "overview",
        markdown: include_str!("../../docs/architecture/overview.md"),
    },
    DocPage {
        title: "Request Flow",
        section: "architecture",
        slug: "request-flow",
        markdown: include_str!("../../docs/architecture/request-flow.md"),
    },
    DocPage {
        title: "Auth and Access",
        section: "architecture",
        slug: "auth-and-access",
        markdown: include_str!("../../docs/architecture/auth-and-access.md"),
    },
    DocPage {
        title: "Data Domains",
        section: "architecture",
        slug: "data-domains",
        markdown: include_str!("../../docs/architecture/data-domains.md"),
    },
    DocPage {
        title: "Database and Schemas",
        section: "architecture",
        slug: "database-and-schemas",
        markdown: include_str!("../../docs/architecture/database-and-schemas.md"),
    },
    DocPage {
        title: "Files and CDN",
        section: "architecture",
        slug: "files-and-cdn",
        markdown: include_str!("../../docs/architecture/files-and-cdn.md"),
    },
    DocPage {
        title: "Integrations",
        section: "architecture",
        slug: "integrations",
        markdown: include_str!("../../docs/architecture/integrations.md"),
    },
    DocPage {
        title: "API Overview",
        section: "api",
        slug: "overview",
        markdown: include_str!("../../docs/api/overview.md"),
    },
    DocPage {
        title: "Auth API",
        section: "api",
        slug: "auth",
        markdown: include_str!("../../docs/api/auth.md"),
    },
    DocPage {
        title: "API Keys API",
        section: "api",
        slug: "api-keys",
        markdown: include_str!("../../docs/api/api-keys.md"),
    },
    DocPage {
        title: "Users API",
        section: "api",
        slug: "users",
        markdown: include_str!("../../docs/api/users.md"),
    },
    DocPage {
        title: "Admin API",
        section: "api",
        slug: "admin",
        markdown: include_str!("../../docs/api/admin.md"),
    },
    DocPage {
        title: "Workflow APIs",
        section: "api",
        slug: "workflows",
        markdown: include_str!("../../docs/api/workflows.md"),
    },
    DocPage {
        title: "Training API",
        section: "api",
        slug: "training",
        markdown: include_str!("../../docs/api/training.md"),
    },
    DocPage {
        title: "Events API",
        section: "api",
        slug: "events",
        markdown: include_str!("../../docs/api/events.md"),
    },
    DocPage {
        title: "Incidents API",
        section: "api",
        slug: "incidents",
        markdown: include_str!("../../docs/api/incidents.md"),
    },
    DocPage {
        title: "Integrations API",
        section: "api",
        slug: "integrations",
        markdown: include_str!("../../docs/api/integrations.md"),
    },
    DocPage {
        title: "Emails API",
        section: "api",
        slug: "emails",
        markdown: include_str!("../../docs/api/emails.md"),
    },
    DocPage {
        title: "Feedback API",
        section: "api",
        slug: "feedback",
        markdown: include_str!("../../docs/api/feedback.md"),
    },
    DocPage {
        title: "Files API",
        section: "api",
        slug: "files",
        markdown: include_str!("../../docs/api/files.md"),
    },
    DocPage {
        title: "Publications API",
        section: "api",
        slug: "publications",
        markdown: include_str!("../../docs/api/publications.md"),
    },
    DocPage {
        title: "Stats API",
        section: "api",
        slug: "stats",
        markdown: include_str!("../../docs/api/stats.md"),
    },
    DocPage {
        title: "Broadcasts API",
        section: "api",
        slug: "broadcasts",
        markdown: include_str!("../../docs/api/broadcasts.md"),
    },
    DocPage {
        title: "Welcome Messages API",
        section: "api",
        slug: "welcome-messages",
        markdown: include_str!("../../docs/api/welcome-messages.md"),
    },
    DocPage {
        title: "Captcha API",
        section: "api",
        slug: "captcha",
        markdown: include_str!("../../docs/api/captcha.md"),
    },
    DocPage {
        title: "Jobs and Sync",
        section: "operations",
        slug: "jobs-and-sync",
        markdown: include_str!("../../docs/operations/jobs-and-sync.md"),
    },
    DocPage {
        title: "Service Accounts",
        section: "operations",
        slug: "service-accounts",
        markdown: include_str!("../../docs/operations/service-accounts.md"),
    },
    DocPage {
        title: "Troubleshooting",
        section: "operations",
        slug: "troubleshooting",
        markdown: include_str!("../../docs/operations/troubleshooting.md"),
    },
    DocPage {
        title: "Code Organization",
        section: "contributors",
        slug: "code-organization",
        markdown: include_str!("../../docs/contributors/code-organization.md"),
    },
    DocPage {
        title: "Adding Routes",
        section: "contributors",
        slug: "adding-routes",
        markdown: include_str!("../../docs/contributors/adding-routes.md"),
    },
    DocPage {
        title: "Documenting Endpoints",
        section: "contributors",
        slug: "documenting-endpoints",
        markdown: include_str!("../../docs/contributors/documenting-endpoints.md"),
    },
];

pub fn find_doc_page(section: Option<&str>, slug: Option<&str>) -> Option<&'static DocPage> {
    match (section, slug) {
        (None, None) => DOC_PAGES.iter().find(|page| page.section.is_empty()),
        (Some(section), Some(slug)) => DOC_PAGES
            .iter()
            .find(|page| page.section == section && page.slug == slug),
        _ => None,
    }
}

pub fn docs_page_links() -> Vec<(&'static str, &'static str, &'static str)> {
    DOC_PAGES
        .iter()
        .filter(|page| !page.section.is_empty())
        .map(|page| (page.section, page.slug, page.title))
        .collect()
}

pub fn render_markdown_page(page: &DocPage) -> String {
    let mut rendered = String::new();
    html::push_html(
        &mut rendered,
        Parser::new_ext(page.markdown, markdown_options()),
    );
    render_docs_shell(page.title, &rendered)
}

fn markdown_options() -> Options {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    options
}

fn render_docs_shell(title: &str, content: &str) -> String {
    let nav = docs_page_links()
        .into_iter()
        .map(|(section, slug, page_title)| {
            format!(r#"<li><a href="/docs/{section}/{slug}">{page_title}</a></li>"#)
        })
        .collect::<Vec<_>>()
        .join("");

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title} | Osmium Docs</title>
  <style>
    :root {{
      --bg: #f4f1e8; --panel: #fffdf8; --ink: #1f2933; --muted: #586574;
      --line: #d9d2c3; --accent: #8b3d2e; --accent-soft: #f1ddd5; --code: #f6efe3; --link: #0d5c63;
    }}
    * {{ box-sizing: border-box; }} body {{ margin: 0; font-family: Georgia, "Palatino Linotype", serif; color: var(--ink);
      background: radial-gradient(circle at top left, #efe6d7 0%, transparent 28%), linear-gradient(180deg, #f8f4ec 0%, var(--bg) 100%); }}
    a {{ color: var(--link); text-decoration: none; }} a:hover {{ text-decoration: underline; }}
    .layout {{ display: grid; grid-template-columns: 300px minmax(0, 1fr); min-height: 100vh; }}
    .sidebar {{ border-right: 1px solid var(--line); background: rgba(255,253,248,.92); padding: 24px 20px 40px; position: sticky; top: 0; height: 100vh; overflow-y: auto; }}
    .sidebar h1 {{ margin: 0 0 8px; font-size: 1.6rem; color: var(--accent); }} .sidebar p {{ margin: 0 0 20px; color: var(--muted); line-height: 1.5; }}
    .sidebar ul {{ list-style: none; margin: 0; padding: 0; }} .sidebar li {{ margin: 0 0 10px; line-height: 1.4; }}
    .content {{ padding: 48px min(6vw, 72px); }} .article {{ max-width: 960px; background: var(--panel); border: 1px solid var(--line); border-radius: 18px; padding: 40px; box-shadow: 0 24px 60px rgba(31,41,51,.08); }}
    .article h1, .article h2, .article h3 {{ color: var(--accent); line-height: 1.2; }} .article p, .article li {{ line-height: 1.7; font-size: 1.03rem; }}
    .article code {{ background: var(--code); padding: .1rem .35rem; border-radius: 6px; font-family: Menlo, monospace; font-size: .92em; }}
    .article pre {{ background: #1f2933; color: #f8fafc; padding: 16px; border-radius: 12px; overflow-x: auto; }} .article pre code {{ background: transparent; padding: 0; color: inherit; }}
    .article blockquote {{ margin: 1rem 0; padding: .6rem 1rem; border-left: 4px solid var(--accent); background: var(--accent-soft); }}
    .article table {{ width: 100%; border-collapse: collapse; margin: 1.25rem 0; }} .article th, .article td {{ border: 1px solid var(--line); padding: .75rem; text-align: left; vertical-align: top; }}
    @media (max-width: 960px) {{ .layout {{ grid-template-columns: 1fr; }} .sidebar {{ position: static; height: auto; border-right: 0; border-bottom: 1px solid var(--line); }} .content {{ padding: 20px; }} .article {{ padding: 24px; }} }}
  </style>
</head>
<body>
  <div class="layout">
    <aside class="sidebar">
      <h1><a href="/docs">Osmium Docs</a></h1>
      <p>Platform docs for developers, maintainers, and internal app or bot consumers.</p>
      <ul>{nav}</ul>
      <p><a href="/docs/api/v1">Interactive API reference</a></p>
    </aside>
    <main class="content"><article class="article">{content}</article></main>
  </div>
</body>
</html>"#
    )
}
