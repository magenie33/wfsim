#!/usr/bin/env python3
"""Install the bounded calibration read with its narrow internal grant."""
import re
import subprocess
from ship_factoryd import ROOT, ssh, target

FILES = ['worker/pg/sync.sql', 'deploy/work_calibration_grants.sql', 'scripts/ship_work_calibration.py']

def main():
    if subprocess.check_output(['git','status','--porcelain','--',*FILES],cwd=ROOT).strip():
        raise RuntimeError('Calibration deployment requires committed sources')
    source = (ROOT/'worker/pg/sync.sql').read_text()
    body = re.search(r'CREATE OR REPLACE FUNCTION factory_calibration_page\([\s\S]*?\$\$;',source)[0]
    signature = 'factory_calibration_page(text,text,text,bigint,bigint,boolean,boolean)'
    sql = "BEGIN;SET LOCAL lock_timeout='1s';SET LOCAL statement_timeout='8s';SET LOCAL ROLE wfsim;\n"
    sql += "SELECT set_config('search_path',quote_ident(current_schema())||',pg_catalog,pg_temp',true);\n"
    sql += body+'\nREVOKE ALL ON FUNCTION '+signature+' FROM PUBLIC;\n'
    sql += (ROOT/'deploy/work_calibration_grants.sql').read_text()+'\nCOMMIT;\n'
    ssh(target(),'sudo -u postgres psql -X -q -v ON_ERROR_STOP=1 -d wfsim',stdin=sql.encode())
    print('Named calibration read installed; pipeline role has only function execution.')

if __name__ == '__main__':
    main()
