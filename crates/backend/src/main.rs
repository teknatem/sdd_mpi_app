//! Точка входа. Вся структура крейта — в `lib.rs`; здесь только стартовая
//! процедура, чтобы интеграционные тесты могли линковаться против библиотеки.

use backend::{api, processes, quality, shared, system, AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use axum::http::{header, Method};
    use axum::middleware;
    use std::net::SocketAddr;
    use tokio::net::TcpListener;
    use tower_http::cors::{Any, CorsLayer};
    use tower_http::services::ServeDir;

    println!("\n");
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║           MARKETPLACE BACKEND STARTING...                ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!("\n");

    // 1. Initialize tracing (системное логирование)
    println!("Step 1: Initializing logging system...");
    match system::tracing::initialize() {
        Ok(_) => println!("✓ Logging system initialized\n"),
        Err(e) => {
            println!("✗ ERROR: Failed to initialize logging: {}\n", e);
            return Err(e);
        }
    }

    // 1.5 Отложенная подмена БД — СТРОГО до открытия пула подключений.
    // Восстановление базы не может подменить файл на живом приложении (его
    // держит пул, и Windows не даст переименовать поверх), поэтому оно лишь
    // готовит файл, а установка происходит здесь. Печатаем заметно: смена
    // содержимого базы не должна выглядеть как что-то, случившееся само.
    match shared::config::load_config() {
        Ok(config) => {
            if let Some(report) = system::datasets::db_restore::apply_pending_restore(&config) {
                println!("Step 1.5: {report}\n");
            }
        }
        Err(e) => println!("⚠ Could not check for a pending database restore: {}\n", e),
    }

    // 2. Initialize database (loads config from config.toml)
    println!("Step 2: Initializing database...");
    let db = match shared::data::db::initialize_database().await {
        Ok(db) => {
            println!("✓ Database initialized successfully\n");
            db
        }
        Err(e) => {
            println!("✗ CRITICAL ERROR: Database initialization failed!");
            println!("   Error: {}\n", e);
            println!("========================================");
            println!("Application cannot start without database.");
            println!("Please check the error messages above.");
            println!("========================================\n");
            return Err(anyhow::anyhow!("db init failed: {e}"));
        }
    };
    let state = AppState::new(db);

    // 3. Run database migrations
    println!("Step 3: Running database migrations...");
    match shared::data::migration_runner::run_migrations().await {
        Ok(_) => println!("✓ Database migrations processed\n"),
        Err(e) => {
            println!("✗ ERROR: Database migrations failed: {}\n", e);
            return Err(e);
        }
    }

    // 3.1 Reset stale Running rows from previous server process (sys_task_runs)
    match system::tasks::runs_service::reset_stale_running_runs("Server restarted").await {
        Ok(n) if n > 0 => println!(
            "✓ Reset {} stale scheduled task run(s) (were Running after restart)\n",
            n
        ),
        Ok(_) => {}
        Err(e) => println!("⚠ Could not reset stale task runs: {}\n", e),
    }

    // 3.2 Приведение файловой раскладки в порядок: подкаталоги [data].root и
    // перенос вложений чатов из каталога, зависевшего от рабочей директории.
    // Обе процедуры идемпотентны и не критичны — расхождения видны на странице
    // «Наборы данных и перенос».
    match shared::config::load_config() {
        Ok(config) => {
            system::datasets::bootstrap::ensure_data_root(&config);
            system::datasets::bootstrap::migrate_legacy_attachments(&config);
        }
        Err(e) => println!("⚠ Could not prepare data directories: {}\n", e),
    }

    // Skills have a single runtime source: the configured external catalog.
    // First startup creates the directory and materializes any missing embedded seeds.
    let skill_snapshot = shared::llm::skills::snapshot();
    if skill_snapshot.skills.is_empty()
        || skill_snapshot
            .diagnostics
            .iter()
            .any(|message| message.starts_with("CRITICAL:"))
    {
        return Err(anyhow::anyhow!(
            "external skill catalog initialization failed: {}",
            skill_snapshot.diagnostics.join("; ")
        ));
    }
    println!(
        "✓ External skill catalog initialized: {} skills\n",
        skill_snapshot.skills.len()
    );

    // 4. Ensure admin user exists
    println!("Step 4: Checking admin user...");
    match system::initialization::ensure_admin_user_exists().await {
        Ok(_) => println!("✓ Admin user verified\n"),
        Err(e) => {
            println!("✗ ERROR: Admin user check failed: {}\n", e);
            return Err(e);
        }
    }

    // 4.1. Scheduled task worker startup mode + external API key
    let scheduled_task_worker_enabled = match shared::config::load_config() {
        Ok(cfg) => {
            shared::config::set_ext_api_key(cfg.external_api.api_key.clone());
            if cfg.external_api.api_key.is_empty() {
                println!("⚠  External API: disabled (api_key not set in config.toml)\n");
            } else {
                println!("✓ External API: enabled (X-Api-Key configured)\n");
            }
            shared::config::set_scheduler_config_enabled(cfg.scheduled_tasks.enabled);
            shared::config::set_mail_config(cfg.mail.clone());
            shared::config::set_bitrix24_config(cfg.bitrix24.clone());
            if cfg.mail.enabled {
                println!(
                    "✓ Mail: enabled (IMAP {}:{}, SMTP {}:{}, user {})\n",
                    cfg.mail.imap_host,
                    cfg.mail.imap_port,
                    cfg.mail.smtp_host,
                    cfg.mail.smtp_port,
                    cfg.mail.username
                );
            } else {
                println!("⚠  Mail: disabled ([mail].enabled not set in config.toml)\n");
            }
            cfg.scheduled_tasks.enabled
        }
        Err(e) => {
            println!(
                "✗ ERROR: Failed to load config for scheduled tasks: {}\n",
                e
            );
            return Err(e);
        }
    };

    let quality_reload = quality::registry::reload().await;
    if quality_reload.ok {
        println!(
            "✓ Quality checks loaded: generation {}\n",
            quality_reload.generation
        );
    } else {
        println!(
            "⚠ Quality checks reload rejected; previous embedded catalog remains active: {:?}\n",
            quality_reload.diagnostics
        );
    }

    println!(
        "Step 5: Scheduled task worker is {} (config.toml -> [scheduled_tasks].enabled)",
        if scheduled_task_worker_enabled {
            "ENABLED"
        } else {
            "DISABLED"
        }
    );

    // Always initialize the task manager registry so that task type metadata
    // is available via /api/sys/tasks/task_types regardless of whether the
    // background scheduler is enabled.
    println!("Step 6: Initializing task manager registry...");
    let worker = match system::tasks::initialization::initialize_scheduled_tasks().await {
        Ok(w) => {
            println!("✓ Task manager registry initialized\n");
            w
        }
        Err(e) => {
            println!(
                "✗ ERROR: Task manager registry initialization failed: {}\n",
                e
            );
            return Err(e);
        }
    };

    if scheduled_task_worker_enabled {
        println!("Step 7: Starting background worker...");
        tokio::spawn(async move {
            worker.run_loop().await;
        });
        println!("✓ Background worker started\n");
    } else {
        println!("Step 7: Background worker disabled by configuration — registry loaded, scheduler inactive\n");
        // Keep `worker` alive (drop at end of main) so the registry Arc stays valid.
        drop(worker);
    }

    // Посев определений пилота: коды заводятся черновиками, если их ещё нет.
    // Активация — решение человека с просмотром плана эффектов (ADR-0011 п.8),
    // поэтому посев ничего не включает.
    match processes::pilot::seed(shared::data::db::get_connection()).await {
        Ok(report) if report.is_empty() => {}
        Ok(report) => println!(
            "✓ Процессы: заведены черновики {:?}{}
",
            report.stages_created,
            if report.process_created {
                " + pr0001"
            } else {
                ""
            }
        ),
        Err(e) => println!(
            "✗ Процессы: посев определений не удался: {e}
"
        ),
    }

    // Воркер экземпляров процессов. Намеренно не под флагом планировщика
    // (ADR-0011 п.12): регламентные задания и Процессы — разные механизмы, и
    // выключенный планировщик не должен означать остановленные Процессы.
    // Пока не активирован ни один Процесс, проход воркера — четыре пустых
    // запроса.
    tokio::spawn(async {
        processes::worker::ProcessWorker::new(30)
            .run_loop(shared::data::db::get_connection())
            .await;
    });

    // Прунинг лога внешнего API. Намеренно не регламентное задание: планировщик
    // может быть выключен в config.toml, и тогда задание просто не выполнилось бы.
    tokio::spawn(async {
        system::ext_api_log::service::run_prune_loop().await;
    });

    // Флаш статистики базы знаний. По той же причине не задание планировщика:
    // счётчики обращений к статьям должны копиться независимо от него.
    shared::llm::kb_metrics::spawn_flusher();

    // Сгенерированные карты базы знаний (профиль данных, плагины, навыки,
    // проверки). Фоново и с задержкой: карты нужны ассистенту, а не первому
    // HTTP-запросу, и обход всех таблиц не должен растягивать старт.
    tokio::spawn(async {
        // Профиль прошлого запуска поднимаем сразу: таблица переживает рестарт,
        // и до пересчёта схема объекта отдавала бы «данных нет» вместо цифр.
        shared::data::data_profile::refresh_snapshot().await;
        tokio::time::sleep(std::time::Duration::from_secs(20)).await;
        if let Some(_database_activity) = system::maintenance::try_begin_database_activity() {
            shared::llm::kb_generated::regenerate_all().await;
        }

        // Снимок метрик проекта — последним в этой же цепочке, а не отдельной
        // задачей: он читает `sys_data_profile`, который обновляет именно
        // `regenerate_all`. Собери раньше — и в снимок уйдут числа строк с
        // прошлого запуска.
        if let Err(error) = system::metrics::collect_and_store("startup").await {
            tracing::warn!("[metrics] снимок при старте не собран: {error}");
        }
    });

    // 5. Configure CORS
    println!("Step 8: Configuring CORS...");
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE, header::ACCEPT, header::AUTHORIZATION]);
    println!("✓ CORS configured\n");

    // 6. Build app with routes
    println!("Step 9: Building application routes...");
    let app = axum::Router::new()
        .merge(system::api::configure_system_routes())
        .merge(api::configure_business_routes())
        .fallback_service(ServeDir::new("dist"))
        // Слои применяются снаружи внутрь в обратном порядке, поэтому логгер
        // оборачивает гейт: отклонённые обслуживанием запросы тоже попадают в
        // журнал. При выключенном режиме гейт стоит один атомарный load.
        .layer(middleware::from_fn(
            system::middleware::maintenance_gate::maintenance_gate,
        ))
        .layer(middleware::from_fn(
            system::middleware::request_logger::request_logger,
        ))
        .layer(cors)
        .with_state(state);
    println!("✓ Routes configured\n");

    // 7. Start server
    println!("Step 10: Starting HTTP server...");
    let addr: SocketAddr = ([0, 0, 0, 0], 3000).into();

    println!("   Attempting to bind to: http://{}", addr);
    tracing::info!("Attempting to bind server to http://{}", addr);

    let listener = match TcpListener::bind(addr).await {
        Ok(listener) => {
            println!("✓ Server successfully bound to port 3000\n");
            tracing::info!("Server successfully bound to {}", addr);

            // Вывод информации о доступе к серверу
            println!("========================================");
            println!("  SERVER ACCESS INFORMATION");
            println!("========================================\n");

            println!("✓ Server is accessible at:\n");
            println!("  📍 Local access (on this computer):");
            println!("     http://localhost:3000");
            println!("     http://127.0.0.1:3000\n");

            println!("  📍 Network access (from other computers):");
            println!("     http://<SERVER-IP>:3000");
            println!("     (replace <SERVER-IP> with this computer's IP address)\n");

            println!("  💡 To find this computer's IP address, run:");
            println!("     ipconfig | findstr IPv4\n");

            println!("⚠  TROUBLESHOOTING: If frontend cannot connect:");
            println!("\n  1. Windows Firewall:");
            println!("     Run PowerShell as Administrator:");
            println!("     New-NetFirewallRule -DisplayName \"Backend Port 3000\" `");
            println!("       -Direction Inbound -LocalPort 3000 -Protocol TCP -Action Allow\n");

            println!("  2. Frontend connection:");
            println!("     • Frontend must open backend at http://<SERVER-IP>:3000");
            println!("     • NOT localhost (unless frontend on same server)");
            println!("     • Check browser console for connection errors\n");

            println!("  3. Check if port is accessible:");
            println!("     From another computer, try:");
            println!("     curl http://<SERVER-IP>:3000/api/health");
            println!("     Or open in browser: http://<SERVER-IP>:3000\n");

            println!("========================================\n");

            listener
        }
        Err(e) => {
            println!("✗ CRITICAL ERROR: Cannot bind to port 3000!");
            println!("   Error: {}", e);
            println!("   Error kind: {:?}\n", e.kind());

            if e.kind() == std::io::ErrorKind::AddrInUse {
                println!("========================================");
                println!("Port 3000 is already in use!");
                println!("\nPossible solutions:");
                println!("  1. Stop the other process using port 3000");
                println!("  2. Check Task Manager for other backend.exe");
                println!("  3. Run: netstat -ano | findstr :3000");
                println!("========================================\n");

                tracing::error!(
                    "Error: Port 3000 is already in use. Please ensure no other process is using this port."
                );
            } else {
                println!("========================================");
                println!("Failed to bind to port!");
                println!("\nPossible causes:");
                println!("  - Firewall blocking the port");
                println!("  - Insufficient permissions");
                println!("  - Network configuration issue");
                println!("========================================\n");

                tracing::error!("Failed to bind to port 3000. Error: {}", e);
            }
            return Err(e.into());
        }
    };

    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║           SERVER STARTED SUCCESSFULLY!                   ║");
    println!("╠══════════════════════════════════════════════════════════╣");
    println!("║  Server listening on: http://{}                ║", addr);
    println!("║  Press Ctrl+C to stop                                    ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!("\n");

    // ConnectInfo нужен, чтобы ext_api_log писал IP вызывающего;
    // без него client_ip был бы всегда NULL.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(system::restart::wait())
    .await?;

    if system::restart::is_requested() {
        tracing::warn!("Server stopped cleanly; waiting for the service supervisor to restart it");
    }

    Ok(())
}
