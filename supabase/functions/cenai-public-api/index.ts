import "jsr:@supabase/functions-js/edge-runtime.d.ts";
import { createClient } from "npm:@supabase/supabase-js@2.57.4";

const db = createClient(Deno.env.get("SUPABASE_URL")!, Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!, { auth:{persistSession:false,autoRefreshToken:false} });
const headers = { "Content-Type":"application/json", "Cache-Control":"no-store", "X-Content-Type-Options":"nosniff", "Access-Control-Allow-Origin":"*", "Access-Control-Allow-Headers":"authorization, apikey, content-type, x-api-key", "Access-Control-Allow-Methods":"GET, OPTIONS" };
const reply = (status:number, body:unknown) => new Response(JSON.stringify(body), {status,headers});
async function sha256(value:string) {
  const digest = await crypto.subtle.digest("SHA-256",new TextEncoder().encode(value));
  return Array.from(new Uint8Array(digest)).map(x=>x.toString(16).padStart(2,"0")).join("");
}
Deno.serve(async (req:Request) => {
  if (req.method==="OPTIONS") return new Response("ok",{headers});
  if (req.method!=="GET") return reply(405,{error:"Only GET requests are supported by this initial public API release."});
  try {
    const {data:flag,error:flagError}=await db.from("admin_feature_flags").select("enabled").eq("key","api.public.enabled").maybeSingle();
    if(flagError) throw flagError;
    if(flag?.enabled!==true) return reply(503,{error:"The CenAI public API is not enabled yet."});
    const rawKey=req.headers.get("x-api-key") || (req.headers.get("authorization")||"").replace(/^Bearer\s+/i,"");
    if(!rawKey.startsWith("cai_live_") || rawKey.length<50) return reply(401,{error:"A valid CenAI API key is required."});
    const keyHash=await sha256(rawKey);
    const {data:key,error:keyError}=await db.from("api_keys").select("id,owner_user_id,scopes,enabled").eq("key_hash",keyHash).maybeSingle();
    if(keyError) throw keyError;
    if(!key || !key.enabled) return reply(401,{error:"Invalid or revoked API key."});
    const {data:quota,error:quotaError}=await db.from("ai_quota_policies").select("daily_request_limit,monthly_token_limit,enabled").eq("owner_user_id",key.owner_user_id).maybeSingle();
    if(quotaError) throw quotaError;
    const policy={daily_request_limit:quota?.daily_request_limit ?? 100,monthly_token_limit:quota?.monthly_token_limit ?? 500000,enabled:quota?.enabled !== false};
    const now=new Date();
    const dayStart=new Date(Date.UTC(now.getUTCFullYear(),now.getUTCMonth(),now.getUTCDate())).toISOString();
    const monthStart=new Date(Date.UTC(now.getUTCFullYear(),now.getUTCMonth(),1)).toISOString();
    if(policy.enabled){
      const {count:dailyCount,error:dailyError}=await db.from("usage_events").select("id",{count:"exact",head:true}).eq("owner_user_id",key.owner_user_id).eq("source","public_api").gte("created_at",dayStart);
      if(dailyError) throw dailyError;
      if((dailyCount||0)>=policy.daily_request_limit) return reply(429,{error:"Daily CenAI API request quota exceeded.",quota:{dailyLimit:policy.daily_request_limit,used:dailyCount||0,resetAt:new Date(Date.UTC(now.getUTCFullYear(),now.getUTCMonth(),now.getUTCDate()+1)).toISOString()}});
      const {data:monthRows,error:monthError}=await db.from("usage_events").select("input_tokens,output_tokens").eq("owner_user_id",key.owner_user_id).eq("source","public_api").gte("created_at",monthStart).limit(10000);
      if(monthError) throw monthError;
      const monthlyTokens=(monthRows||[]).reduce((n,row)=>n+Number(row.input_tokens||0)+Number(row.output_tokens||0),0);
      if(monthlyTokens>=policy.monthly_token_limit) return reply(429,{error:"Monthly CenAI API token quota exceeded.",quota:{monthlyTokenLimit:policy.monthly_token_limit,usedTokens:monthlyTokens,resetAt:new Date(Date.UTC(now.getUTCFullYear(),now.getUTCMonth()+1,1)).toISOString()}});
    }
    const sinceMinute=new Date(Date.now()-60_000).toISOString();
    const {count:recentCount,error:rateError}=await db.from("usage_events").select("id",{count:"exact",head:true}).eq("api_key_id",key.id).gte("created_at",sinceMinute);
    if(rateError)throw rateError;
    if((recentCount||0)>=60)return reply(429,{error:"Rate limit exceeded. Maximum 60 requests per minute per API key."});
    const path=new URL(req.url).pathname;
    if(path.endsWith("/v1/models")) {
      if(!key.scopes?.includes("models:read")) return reply(403,{error:"API key lacks models:read scope."});
      const {data:models,error}=await db.from("model_registry").select("slug,provider,model_id,display_name,description,tasks,modalities").eq("enabled",true).eq("public",true).order("provider").order("slug");
      if(error) throw error;
      await db.from("api_keys").update({last_used_at:new Date().toISOString()}).eq("id",key.id);
      await db.from("usage_events").insert({owner_user_id:key.owner_user_id,api_key_id:key.id,source:"public_api",event_type:"models.list",status:"success"});
      return reply(200,{object:"list",data:(models||[]).map(m=>({id:m.slug,object:"model",owned_by:m.provider,model:m.model_id,name:m.display_name,description:m.description,tasks:m.tasks,modalities:m.modalities}))});
    }
    if(path.endsWith("/v1/usage")) {
      if(!key.scopes?.includes("usage:read")) return reply(403,{error:"API key lacks usage:read scope."});
      const since=new Date(Date.now()-30*86400000).toISOString();
      const {data:events,error}=await db.from("usage_events").select("event_type,input_tokens,output_tokens,status,created_at").eq("owner_user_id",key.owner_user_id).gte("created_at",since).order("created_at",{ascending:false}).limit(1000);
      if(error) throw error;
      const rows=events||[];
      await db.from("api_keys").update({last_used_at:new Date().toISOString()}).eq("id",key.id);
      await db.from("usage_events").insert({owner_user_id:key.owner_user_id,api_key_id:key.id,source:"public_api",event_type:"usage.read",status:"success"});
      return reply(200,{period_days:30,total_events:rows.length,successful:rows.filter(x=>x.status==="success").length,failed:rows.filter(x=>x.status==="error").length,input_tokens:rows.reduce((n,x)=>n+(x.input_tokens||0),0),output_tokens:rows.reduce((n,x)=>n+(x.output_tokens||0),0),truncated:rows.length===1000});
    }
    return reply(404,{error:"Endpoint not found.",available:["GET /v1/models","GET /v1/usage"]});
  } catch(error) {
    console.error("CenAI public API error",error instanceof Error?error.message:"unknown");
    return reply(500,{error:"CenAI API request failed."});
  }
});
