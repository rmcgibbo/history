use anyhow::Result;
use rusqlite::Connection;

pub fn create_schema(con: &Connection) -> Result<()> {
    con.execute_batch(
        "
        create table if not exists commands (
            id integer primary key autoincrement,
            argv text,
            unique(argv) on conflict ignore
        );
        create table if not exists places (
            id integer primary key autoincrement,
            host text,
            dir text,
            unique(host, dir) on conflict ignore
        );
        create table if not exists history (
            id integer primary key autoincrement,
            session int,
            command_id int references commands (id),
            place_id int references places (id),
            exit_status int,
            end_time int);

        PRAGMA user_version = 1;
        PRAGMA journal_mode = WAL;
        PRAGMA locking_mode = EXCLUSIVE;
        PRAGMA synchronous = normal;

        create index if not exists hist_time on history(end_time);
        create index if not exists place_dir on places(dir);
        create index if not exists place_host on places(host);
        create index if not exists history_command_place on history(command_id, place_id);
",
    )?;

    Ok(())
}
