// The official calibration role can execute the named read, not select task tables.
export async function calibrationReader(url = process.env.PG_URL) {
  if (!url) throw new Error('PG_URL is required for the named calibration read');
  const { default: postgres } = await import('./pg/node_modules/postgres/src/index.js');
  const sql = postgres(url, { max: 1, fetch_types: false, onnotice: () => {}, connect_timeout: 15,
    connection: { application_name: 'wfsim-work-calibration', statement_timeout: 8000 } });
  const page = (since, until, after, limit, details = false, inclusive = false) => sql.begin('read only', async tx => {
    await tx.unsafe("SET LOCAL lock_timeout='1s'");
    const [row] = await tx.unsafe('SELECT factory_calibration_page($1::text,$2::text,$3::text,0,$4::bigint,$5::boolean,$6::boolean) AS rows',
      [since, until, after, limit, details, inclusive]);
    return row.rows;
  });
  return { page, close: () => sql.end({ timeout: 5 }) };
}
