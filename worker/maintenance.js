// The maintenance fence runs before database handles or scheduled jobs are opened.
export const databaseMaintenance=env=>env.DATABASE_MAINTENANCE==="1";
export function maintenanceResponse() {
  return new Response(JSON.stringify({ok:false,maintenance:true,retryable:true,error:"Scheduled maintenance. Please retry shortly."}),
    {status:503,headers:{"content-type":"application/json; charset=utf-8","cache-control":"no-store","retry-after":"60"}});
}
