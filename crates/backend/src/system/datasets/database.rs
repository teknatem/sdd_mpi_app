//! Набор `database`: снимок файла БД, его выгрузка в S3 и обратная загрузка.
//!
//! Три вещи, которые отличают этот набор от файловых:
//!
//! 1. **Снимок нельзя снять простым копированием.** Живой `app.db` в режиме WAL
//!    консистентен только вместе с `-wal`; копия, снятая под нагрузкой, — мусор.
//!    Поэтому снимок делает `VACUUM INTO`: SQLite сам собирает целостный и
//!    уплотнённый файл.
//! 2. **Объект не помещается в память.** Боевая база — гигабайты, поэтому и
//!    выгрузка, и загрузка идут потоком: файл → gzip → части по 64 МиБ → S3, и
//!    в обратную сторону. Ни на одном шаге весь объект в память не читается.
//! 3. **Подменить файл на живом приложении нельзя** — его держит пул
//!    подключений. Восстановление только готовит подмену (см. `db_restore`).

use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use bytes::Bytes;
use contracts::system::datasets::DbSetStats;
use flate2::write::GzEncoder;
use flate2::Compression as GzLevel;
use futures_util::StreamExt;
use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::Row;

use super::jobs::JobHandle;
use crate::shared::config::S3Config;
use crate::shared::config::{self, Config};
use crate::shared::data::db::get_connection;
use crate::shared::data::raw_storage;
use crate::system::s3::client::{self, CompletedPart};

/// Имя файла БД внутри набора. Единственная запись в дереве файлов набора —
/// по ней считается дифф и показывается сравнение размеров в UI.
pub const DB_ENTRY_NAME: &str = "app.db";

/// Сколько читать за раз при сжатии и при распаковке.
const IO_CHUNK: usize = 1024 * 1024;

/// Результат снятия снимка: файл на диске плюс то, что о нём надо написать в манифест.
pub struct DbSnapshotFile {
    pub path: PathBuf,
    /// Размер несжатого снимка.
    pub size_bytes: u64,
    pub stats: Option<DbSetStats>,
}

impl Drop for DbSnapshotFile {
    /// Снимок — временный файл в несколько гигабайт: оставлять его после любого
    /// исхода нельзя, поэтому уборка привязана к владению, а не к happy path.
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Результат выгрузки объекта в S3.
pub struct UploadedObject {
    /// Размер и sha256 того, что реально легло в бакет (сжатого потока) —
    /// по ним объект проверяется при скачивании.
    pub size_bytes: u64,
    pub sha256: String,
    /// sha256 исходного, несжатого снимка: идёт в `FileEntry` набора и участвует
    /// в диффе. Считается на том же проходе чтения — второй проход по файлу в
    /// несколько гигабайт стоил бы дороже, чем вся остальная подготовка.
    pub plain_sha256: String,
}

/// SQLite ждёт путь в одинарных кавычках; внутри кавычка удваивается.
/// Обратные слэши Windows SQLite понимает, но прямые безопаснее в литерале.
fn sql_path_literal(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    format!("'{}'", text.replace('\'', "''"))
}

/// Куда класть промежуточные файлы: общий `tmp` каталога данных, а не системный
/// temp — на снимок нужны гигабайты, и они должны лечь на том с данными.
fn tmp_dir(config: &Config) -> anyhow::Result<PathBuf> {
    let dir = config::get_tmp_path(config);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Снимает целостный снимок БД в отдельный файл.
///
/// Порядок шагов не случаен: сначала WAL сводится в основной файл, затем
/// проверяется целостность источника (битую базу незачем ни выгружать, ни
/// хранить как «резервную копию»), и только потом делается `VACUUM INTO`.
/// Полученный файл проверяется ещё раз — уже как самостоятельная база.
pub async fn snapshot_to_file(
    config: &Config,
    snapshot_id: &str,
    job: &JobHandle,
) -> anyhow::Result<DbSnapshotFile> {
    job.check_cancelled()?;

    let stats = raw_storage::vacuum_status()
        .await
        .ok()
        .map(|status| DbSetStats {
            file_mb: status.file_mb,
            reclaimable_mb: status.reclaimable_mb,
            wal_mb: status.wal_mb,
        });

    let _ = raw_storage::truncate_wal().await;
    let source_check_started = Instant::now();
    raw_storage::integrity_check().await.map_err(|error| {
        anyhow::anyhow!("Локальная база не прошла проверку целостности: {error}")
    })?;
    tracing::info!(
        "datasets: source database integrity_check finished in {:.1}s",
        source_check_started.elapsed().as_secs_f64()
    );

    let path = tmp_dir(config)?.join(format!("db-snapshot-{snapshot_id}.db"));
    // VACUUM INTO отказывается писать в существующий файл.
    let _ = std::fs::remove_file(&path);

    let vacuum_started = Instant::now();
    if let Err(error) = get_connection()
        .execute(Statement::from_string(
            DatabaseBackend::Sqlite,
            format!("VACUUM INTO {}", sql_path_literal(&path)),
        ))
        .await
    {
        // Прерванный VACUUM оставляет частично записанный файл на гигабайты:
        // владелец (DbSnapshotFile) тут ещё не создан, убираем вручную.
        let _ = std::fs::remove_file(&path);
        anyhow::bail!(
            "VACUUM INTO не удался: {error}. Проверьте свободное место на томе с каталогом данных."
        );
    }
    tracing::info!(
        "datasets: VACUUM INTO finished in {:.1}s",
        vacuum_started.elapsed().as_secs_f64()
    );

    // С этого момента файл принадлежит DbSnapshotFile и будет убран в любом исходе.
    let mut snapshot = DbSnapshotFile {
        path,
        size_bytes: 0,
        stats,
    };

    job.set_stage(super::jobs::stage::SNAPSHOT_DB_VERIFY);
    let snapshot_check_started = Instant::now();
    verify_snapshot_file(&snapshot.path).await?;
    tracing::info!(
        "datasets: snapshot integrity_check finished in {:.1}s",
        snapshot_check_started.elapsed().as_secs_f64()
    );
    snapshot.size_bytes = std::fs::metadata(&snapshot.path)?.len();
    Ok(snapshot)
}

/// `PRAGMA integrity_check` на отдельном файле: подключаемся к нему как к
/// самостоятельной базе в режиме только для чтения.
///
/// Путь задаётся через `filename`, а не строкой `sqlite://…`: на Windows буква
/// диска в URL разбирается как хост (`F:` → host `f`), и подключение уезжает не
/// туда. Тот же приём в `shared/data/db.rs`.
async fn verify_snapshot_file(path: &Path) -> anyhow::Result<()> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .create_if_missing(false);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|error| anyhow::anyhow!("Не удалось открыть снимок БД для проверки: {error}"))?;

    let rows = sqlx::query("PRAGMA integrity_check")
        .fetch_all(&pool)
        .await
        .map_err(|error| anyhow::anyhow!("Проверка целостности снимка не выполнилась: {error}"))?;
    let messages: Vec<String> = rows
        .iter()
        .filter_map(|row| row.try_get::<String, _>(0).ok())
        .collect();
    pool.close().await;

    if messages.len() == 1 && messages[0] == "ok" {
        return Ok(());
    }
    anyhow::bail!(
        "Снимок БД не прошёл проверку целостности: {}",
        if messages.is_empty() {
            "СУБД не вернула результат".to_string()
        } else {
            messages.join("; ")
        }
    )
}

// ---------------------------------------------------------------------------
// Выгрузка
// ---------------------------------------------------------------------------

/// `io::Write`, который режет поток на части и отдаёт их загрузчику.
///
/// Канал ограничен намеренно: сжатие идёт быстрее сети, и без предела очередь
/// готовых частей съела бы память ровно тем объёмом, которого мы избегаем.
struct PartSink {
    tx: tokio::sync::mpsc::Sender<Vec<u8>>,
    buffer: Vec<u8>,
    part_size: usize,
}

impl PartSink {
    fn flush_part(&mut self) -> std::io::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let part = std::mem::replace(&mut self.buffer, Vec::with_capacity(self.part_size));
        self.tx.blocking_send(part).map_err(|_| {
            // Приёмник закрылся: загрузка отменена или упала.
            std::io::Error::new(std::io::ErrorKind::BrokenPipe, "загрузка прервана")
        })
    }
}

impl Write for PartSink {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.buffer.extend_from_slice(data);
        while self.buffer.len() >= self.part_size {
            let rest = self.buffer.split_off(self.part_size);
            let part = std::mem::replace(&mut self.buffer, rest);
            self.tx.blocking_send(part).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::BrokenPipe, "загрузка прервана")
            })?;
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Сжимает файл gzip'ом и выгружает его в S3 многочастной загрузкой.
///
/// Прогресс считается по **прочитанным** байтам исходного файла: его объём
/// известен заранее, в отличие от объёма после сжатия. Конвейер лок-степовый
/// (канал на две части), поэтому чтение не убегает от сети.
pub async fn compress_and_upload(
    cfg: &S3Config,
    key: &str,
    source: &Path,
    job: &JobHandle,
) -> anyhow::Result<UploadedObject> {
    let started = Instant::now();
    job.check_cancelled()?;
    let total = std::fs::metadata(source)?.len();
    job.set_bytes_total(total);

    let part_size = client::part_size_for(total);
    let upload_id = client::create_multipart_upload(cfg, key, Some("application/gzip")).await?;

    let outcome = pump_parts(cfg, key, &upload_id, source, part_size, job).await;

    match outcome {
        Ok((parts, uploaded)) => {
            match client::complete_multipart_upload(cfg, key, &upload_id, &parts).await {
                Ok(()) => {
                    tracing::info!(
                        "datasets: database gzip + multipart upload finished in {:.1}s ({:.1} MiB)",
                        started.elapsed().as_secs_f64(),
                        uploaded.size_bytes as f64 / 1024.0 / 1024.0
                    );
                    Ok(uploaded)
                }
                Err(error) => {
                    let _ = client::abort_multipart_upload(cfg, key, &upload_id).await;
                    Err(error)
                }
            }
        }
        Err(error) => {
            // Незавершённая загрузка остаётся в бакете невидимой и платной —
            // отменяем её в любой ветке ошибки, включая отмену пользователем.
            if let Err(abort_error) = client::abort_multipart_upload(cfg, key, &upload_id).await {
                tracing::warn!(
                    "datasets: не удалось отменить многочастную загрузку: {abort_error}"
                );
            }
            Err(error)
        }
    }
}

async fn pump_parts(
    cfg: &S3Config,
    key: &str,
    upload_id: &str,
    source: &Path,
    part_size: usize,
    job: &JobHandle,
) -> anyhow::Result<(Vec<CompletedPart>, UploadedObject)> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(2);
    let source = source.to_path_buf();
    let reader_job = job.clone();

    let reader = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
        let mut file = std::fs::File::open(&source)?;
        let sink = PartSink {
            tx,
            buffer: Vec::with_capacity(part_size),
            part_size,
        };
        let mut encoder = GzEncoder::new(sink, GzLevel::default());
        let mut plain = Sha256::new();
        let mut buffer = vec![0_u8; IO_CHUNK];
        loop {
            reader_job.check_cancelled()?;
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            plain.update(&buffer[..read]);
            encoder.write_all(&buffer[..read])?;
            reader_job.add_bytes(read as u64);
        }
        // finish() дописывает хвост gzip, после чего остаток буфера уходит
        // последней частью — она может быть меньше минимальной, это разрешено.
        let mut sink = encoder.finish()?;
        sink.flush_part()?;
        Ok(format!("{:x}", plain.finalize()))
    });

    let mut parts = Vec::new();
    let mut hasher = Sha256::new();
    let mut uploaded_bytes = 0_u64;
    let mut part_number = 1_u32;
    let mut failure: Option<anyhow::Error> = None;

    while let Some(part) = rx.recv().await {
        if let Err(error) = job.check_cancelled() {
            failure = Some(error);
            break;
        }
        hasher.update(&part);
        uploaded_bytes += part.len() as u64;
        match client::upload_part(cfg, key, upload_id, part_number, Bytes::from(part)).await {
            Ok(completed) => parts.push(completed),
            Err(error) => {
                failure = Some(error);
                break;
            }
        }
        part_number += 1;
    }
    // Закрываем приём: пишущий поток, если он ещё жив, упрётся в BrokenPipe и завершится.
    rx.close();

    let reader_result = reader.await;
    if let Some(error) = failure {
        return Err(error);
    }
    let plain_sha256 = match reader_result {
        Ok(Ok(sha)) => sha,
        Ok(Err(error)) => return Err(error),
        Err(error) => anyhow::bail!("Поток сжатия завершился аварийно: {error}"),
    };

    Ok((
        parts,
        UploadedObject {
            size_bytes: uploaded_bytes,
            sha256: format!("{:x}", hasher.finalize()),
            plain_sha256,
        },
    ))
}

// ---------------------------------------------------------------------------
// Загрузка
// ---------------------------------------------------------------------------

/// Скачивает объект снимка БД, одновременно сверяет sha256 и распаковывает его
/// в отдельный временный файл. Файл допускается к проверке SQLite и staging
/// только после совпадения контрольной суммы всего сжатого объекта.
pub async fn download_and_unpack(
    cfg: &S3Config,
    config: &Config,
    object_key: &str,
    expected_sha256: &str,
    snapshot_id: &str,
    job: &JobHandle,
) -> anyhow::Result<PathBuf> {
    let transfer_started = Instant::now();
    job.check_cancelled()?;
    let dir = tmp_dir(config)?;
    let unpacked = dir.join(format!("db-restore-{snapshot_id}.db"));
    let _ = std::fs::remove_file(&unpacked);

    let outcome =
        download_verified_and_unpack(cfg, object_key, expected_sha256, &unpacked, job).await;
    if let Err(error) = outcome {
        let _ = std::fs::remove_file(&unpacked);
        return Err(error);
    }
    tracing::info!(
        "datasets: database download + gzip unpack finished in {:.1}s",
        transfer_started.elapsed().as_secs_f64()
    );

    job.set_stage(super::jobs::stage::RESTORE_VERIFY);
    let check_started = Instant::now();
    if let Err(error) = verify_snapshot_file(&unpacked).await {
        let _ = std::fs::remove_file(&unpacked);
        return Err(error);
    }
    tracing::info!(
        "datasets: restored database integrity_check finished in {:.1}s",
        check_started.elapsed().as_secs_f64()
    );
    Ok(unpacked)
}

/// Синхронный `Read` поверх ограниченного async-канала. Декодер gzip работает
/// в `spawn_blocking`, а загрузчик подаёт ему сетевые чанки без промежуточного
/// файла `.gz`. Ограниченный канал не позволяет сети раздувать память.
struct ChannelReader {
    rx: tokio::sync::mpsc::Receiver<Bytes>,
    current: Bytes,
    offset: usize,
}

impl Read for ChannelReader {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        while self.offset == self.current.len() {
            let Some(next) = self.rx.blocking_recv() else {
                return Ok(0);
            };
            self.current = next;
            self.offset = 0;
        }
        let count = output.len().min(self.current.len() - self.offset);
        output[..count].copy_from_slice(&self.current[self.offset..self.offset + count]);
        self.offset += count;
        Ok(count)
    }
}

async fn download_verified_and_unpack(
    cfg: &S3Config,
    object_key: &str,
    expected_sha256: &str,
    dest: &Path,
    job: &JobHandle,
) -> anyhow::Result<()> {
    let (content_length, response) = client::get_object_stream(cfg, object_key).await?;
    job.set_bytes_total(content_length.unwrap_or(0));

    let (tx, rx) = tokio::sync::mpsc::channel::<Bytes>(8);
    let dest = dest.to_path_buf();
    let decoder_job = job.clone();
    let decoder = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let reader = ChannelReader {
            rx,
            current: Bytes::new(),
            offset: 0,
        };
        let mut decoder = flate2::read::GzDecoder::new(reader);
        let mut file = BufWriter::new(std::fs::File::create(dest)?);
        let mut buffer = vec![0_u8; IO_CHUNK];
        loop {
            decoder_job.check_cancelled()?;
            let read = decoder.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            file.write_all(&buffer[..read])?;
        }
        file.flush()?;
        Ok(())
    });

    let mut hasher = Sha256::new();
    let mut stream = response.bytes_stream();
    let mut transfer_error = None;
    while let Some(chunk) = stream.next().await {
        if let Err(error) = job.check_cancelled() {
            transfer_error = Some(error);
            break;
        }
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(error) => {
                transfer_error = Some(error.into());
                break;
            }
        };
        hasher.update(&chunk);
        job.add_bytes(chunk.len() as u64);
        if tx.send(chunk).await.is_err() {
            transfer_error = Some(anyhow::anyhow!(
                "Поток распаковки завершился раньше загрузки"
            ));
            break;
        }
    }
    drop(tx);

    let decoder_result = decoder.await;
    if let Some(error) = transfer_error {
        return Err(error);
    }
    match decoder_result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => return Err(error),
        Err(error) => anyhow::bail!("Поток распаковки завершился аварийно: {error}"),
    }

    let actual = format!("{:x}", hasher.finalize());
    if actual != expected_sha256 {
        anyhow::bail!(
            "Контрольная сумма снимка БД не совпала с манифестом (ожидалась {expected_sha256}, \
             получена {actual}). Объект повреждён или подменён."
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Локальное состояние
// ---------------------------------------------------------------------------

/// Размер и sha256 текущего файла БД — для диффа при восстановлении.
///
/// sha256 живого файла заведомо не совпадёт с sha снимка (`VACUUM INTO`
/// перестраивает страницы), поэтому дифф по базе всегда показывает «изменится».
/// Это честно: содержимое действительно другое.
pub fn local_file_size(config: &Config) -> Option<u64> {
    let path = config::resolve_database_path(config).ok()?.path;
    std::fs::metadata(path).ok().map(|meta| meta.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_path_literal_escapes_quotes_and_backslashes() {
        let path = PathBuf::from(r"F:\data\it's\db.sqlite");
        assert_eq!(sql_path_literal(&path), "'F:/data/it''s/db.sqlite'");
    }

    #[tokio::test]
    async fn channel_reader_joins_stream_chunks_without_losing_bytes() {
        let (tx, rx) = tokio::sync::mpsc::channel(2);
        tx.send(Bytes::from_static(b"abc")).await.unwrap();
        tx.send(Bytes::from_static(b"defgh")).await.unwrap();
        drop(tx);

        let actual = tokio::task::spawn_blocking(move || {
            let mut reader = ChannelReader {
                rx,
                current: Bytes::new(),
                offset: 0,
            };
            let mut output = Vec::new();
            reader.read_to_end(&mut output).unwrap();
            output
        })
        .await
        .unwrap();

        assert_eq!(actual, b"abcdefgh");
    }
}
