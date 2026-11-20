/* Gossil provider regression: compile against the actual official 0.10 header. */
#include "sqlite3.h"
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

_Static_assert(SQLITE_VERSION_NUMBER == 3047000, "official ffi 0.10 SQLite identity");
_Static_assert(sizeof(sqlite3_int64) == sizeof(int64_t), "64-bit SQL integer ABI");
_Static_assert(offsetof(libsql_wal_methods, xSavepointForget) ==
               offsetof(libsql_wal_methods, xDb) +
               sizeof(((libsql_wal_methods *)0)->xDb), "ABI2 callback follows xDb");
_Static_assert(sizeof(libsql_wal_methods) ==
               offsetof(libsql_wal_methods, xSavepointForget) +
               sizeof(((libsql_wal_methods *)0)->xSavepointForget), "ABI2 trailing field");

static void (*original_forget)(wal_impl *, unsigned int *);
static unsigned int forget_calls;
static void counting_forget(wal_impl *wal, unsigned int *data) {
  ++forget_calls;
  original_forget(wal, data);
}

static void require(int condition, const char *message) {
  if (!condition) {
    fprintf(stderr, "%s\n", message);
    exit(1);
  }
}

static void sql(sqlite3 *db, const char *text) {
  char *message = NULL;
  int rc = sqlite3_exec(db, text, NULL, NULL, &message);
  if (rc != SQLITE_OK) {
    fprintf(stderr, "SQL [%s]: %d %s\n", text, rc,
            message != NULL ? message : sqlite3_errmsg(db));
    sqlite3_free(message);
    exit(1);
  }
}

static void scalar(sqlite3 *db, const char *text, const char *expected) {
  sqlite3_stmt *statement = NULL;
  require(sqlite3_prepare_v2(db, text, -1, &statement, NULL) == SQLITE_OK, text);
  require(sqlite3_step(statement) == SQLITE_ROW, text);
  const unsigned char *actual = sqlite3_column_text(statement, 0);
  if (actual == NULL || strcmp((const char *)actual, expected) != 0) {
    fprintf(stderr, "SQL [%s]: wanted [%s], got [%s]\n", text, expected,
            actual != NULL ? (const char *)actual : "NULL");
    exit(1);
  }
  require(sqlite3_step(statement) == SQLITE_DONE, "exactly one scalar row");
  require(sqlite3_finalize(statement) == SQLITE_OK, "scalar finalize");
}

int main(int argc, char **argv) {
  sqlite3 *db = NULL;
  libsql_wal_methods *methods = NULL;
  require(argc == 2, "database path argument");
  require(strcmp(sqlite3_libversion(), SQLITE_VERSION) == 0, "header/runtime version");
  require(strcmp(sqlite3_sourceid(), SQLITE_SOURCE_ID) == 0, "header/runtime source ID");
  require(sqlite3_open(argv[1], &db) == SQLITE_OK, "open real file-backed database");
  scalar(db, "PRAGMA journal_mode=WAL", "wal");
  sql(db, "CREATE TABLE state(value INTEGER NOT NULL)");
  require(sqlite3_file_control(db, "main", SQLITE_FCNTL_WAL_METHODS_POINTER,
                              &methods) == SQLITE_OK, "obtain actual WAL methods");
  require(methods != NULL && methods->iVersion == 2, "actual WAL ABI version two");
  require(methods->xSavepointForget != NULL, "actual WAL forget callback present");
  original_forget = methods->xSavepointForget;
  methods->xSavepointForget = counting_forget;

  sql(db, "BEGIN IMMEDIATE; INSERT INTO state VALUES(1);"
          "SAVEPOINT outer_scope; INSERT INTO state VALUES(2);"
          "SAVEPOINT inner_scope; INSERT INTO state VALUES(3)");
  unsigned int before = forget_calls;
  sql(db, "RELEASE inner_scope");
  require(forget_calls > before, "RELEASE invokes and forwards ABI2 callback");
  sql(db, "ROLLBACK TO outer_scope; RELEASE outer_scope; COMMIT");
  scalar(db, "SELECT group_concat(value, ',') FROM state", "1");
  sql(db, "BEGIN IMMEDIATE; SAVEPOINT keep; INSERT INTO state VALUES(4);"
          "RELEASE keep; COMMIT");
  scalar(db, "SELECT group_concat(value, ',') FROM (SELECT value FROM state ORDER BY value)",
         "1,4");
  methods->xSavepointForget = original_forget;

  /* Date-only closure plus legacy SQL numeric conversion remain separate APIs. */
  scalar(db, "SELECT julianday('2451544.5') = 2451544.5", "1");
  scalar(db, "SELECT julianday('1e999') IS NULL", "1");
  scalar(db, "SELECT julianday('1e-999') = 0.0", "1");
  scalar(db, "SELECT strftime('%Y-%m-%d %H:%M:%f', '2000-01-01 00:00:00.125')",
         "2000-01-01 00:00:00.125");
  scalar(db, "SELECT CAST('12.5' AS REAL) = 12.5", "1");
  scalar(db, "SELECT 9223372036854775807", "9223372036854775807");
  require(sqlite3_close(db) == SQLITE_OK, "close committed WAL database");
  db = NULL;
  require(sqlite3_open(argv[1], &db) == SQLITE_OK, "reopen real database");
  scalar(db, "SELECT group_concat(value, ',') FROM (SELECT value FROM state ORDER BY value)",
         "1,4");
  scalar(db, "PRAGMA integrity_check", "ok");
  require(sqlite3_close(db) == SQLITE_OK, "close reopened database");
  puts("sqlite-3.47.0 wal-abi-2 savepoint-forget numeric-ok");
  return 0;
}
