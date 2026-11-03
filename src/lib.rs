pub mod types;
pub use types::*;

pub mod activity;
pub mod api;
pub mod cloud_client;
pub mod config;
pub mod db;
pub mod discovery;
pub mod firewall;
pub mod logger;
pub mod mode;
pub mod native_lock;
pub mod policy;
pub mod scheduler;
pub mod session;
pub mod tray_ui;
pub mod users;

mod error;
pub use error::{AppError, AppResult};

#[cfg(test)]
mod template_tests {
    use minijinja::{path_loader, Environment, UndefinedBehavior};

    #[test]
    fn root_templates_render_strict() {
        let mut env = Environment::new();
        // Тот же загрузчик, что и у сервера: include резолвятся от корня web/templates
        env.set_loader(path_loader("web/templates"));
        env.set_undefined_behavior(UndefinedBehavior::Strict);

        // Проверяем только шаблоны, которые сервер рендерит целиком.
        // Партиалы проверяются косвенно — через include из этих файлов.
        let roots = ["index.html"]; // добавьте "base.html", если он рендерится отдельно

        for root in roots {
            let tmpl = env
                .get_template(root)
                .unwrap_or_else(|e| panic!("template {root} not found: {e}"));
            if let Err(e) = tmpl.render(minijinja::context! {}) {
                panic!("template {root} failed: {e:#}");
            }
        }
    }
}
