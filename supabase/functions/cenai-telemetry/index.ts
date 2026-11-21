import "jsr:@supabase/functions-js/edge-runtime.d.ts";
import { createClient } from "npm:@supabase/supabase-js@2.57.4";
const url=Deno.env.get("SUPABASE_URL")!;
const anon=Deno.env.get("SUPABASE_ANON_KEY")!;
const service=Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!;
const authDb=createClient(url,anon,{auth:{persistSession:false,autoRefreshToken:false}});
const db=createClient(url,service,{auth:{persistSession:false,autoRefreshToken:false}});
const json=(status:number,body:unknown)=>new Response(JSON.stringify(body),{status,headers:{"Content-Type":"application/json","Cache-Control":"no-store"}});
const fail=(status:number,error:string)=>json(status,{error});
async function registrationAllowed(user: any) {
  const { data: policy, error } = await db.from("registration_policy").select("enforced_from").eq("id", 1).maybeSingle();
  if (error) throw error;
  if (!policy?.enforced_from) throw new Error("Discord registration policy is not configured");
  const createdAt = Date.parse(String(user.created_at ?? ""));
  const enforcedFrom = Date.parse(String(policy.enforced_from));
  const legacy = Number.isFinite(createdAt) && Number.isFinite(enforcedFrom) && createdAt < enforcedFrom;
  const discord = user.app_metadata?.cenai_registration_verified === true && user.app_metadata?.cenai_registration_method === "discord";
  return legacy || discord;
}

async function userFor(req:Request){
 const token=(req.headers.get("Authorization")||"").replace(/^Bearer\s+/i,"");
 if(!token)return null;
 const {data,error}=await authDb.auth.getUser(token);
 return error||!data.user||data.user.is_anonymous?null:data.user;
}
async function enabledFlag(key:string){
 const {data,error}=await db.from("admin_feature_flags").select("enabled").eq("key",key).maybeSingle();
 if(error)throw error;
 return data?.enabled===true;
}
Deno.serve(async(req:Request)=>{
 if(req.method!=="POST")return fail(405,"POST required");
 try{
  const user=await userFor(req);if(!user)return fail(401,"Sign in required");
  if(!await registrationAllowed(user))return fail(403,"Complete Discord invitation registration before using CenAI.");
  const body=await req.json().catch(()=>({}));
  const action=String(body.action||"");
  if(action==="consent"){
    const usage=body.usageAnalytics===true,guardian=body.guardian===true;
    const {error}=await db.from("user_telemetry_consents").upsert({user_id:user.id,usage_analytics_enabled:usage,guardian_enabled:guardian,updated_at:new Date().toISOString()},{onConflict:"user_id"});
    if(error)throw error;
    return json(200,{saved:true,usageAnalytics:usage,guardian});
  }
  const {data:consent,error:consentError}=await db.from("user_telemetry_consents").select("usage_analytics_enabled,guardian_enabled").eq("user_id",user.id).maybeSingle();
  if(consentError)throw consentError;
  if(action==="usage"){
    if(consent?.usage_analytics_enabled!==true || !await enabledFlag("analytics.usage.enabled"))return fail(403,"Usage analytics consent or platform setting is disabled");
    const provider=String(body.provider||"").slice(0,40),model=String(body.model||"").slice(0,160);
    const status=["success","error","cancelled","blocked"].includes(body.status)?body.status:"success";
    const {error}=await db.from("usage_events").insert({owner_user_id:user.id,source:"android",event_type:"chat.request",provider,model_id:model,input_tokens:Math.max(0,Math.min(1000000,Number(body.inputTokens)||0)),output_tokens:Math.max(0,Math.min(1000000,Number(body.outputTokens)||0)),latency_ms:Math.max(0,Math.min(3600000,Number(body.latencyMs)||0)),status,error_code:String(body.errorCode||"").slice(0,80),metadata:{}});
    if(error)throw error;
    return json(200,{recorded:true});
  }
  if(action==="guardian"){
    if(consent?.guardian_enabled!==true || !await enabledFlag("guardian.ingestion.enabled"))return fail(403,"Guardian cloud reporting consent or platform setting is disabled");
    const {error}=await db.from("guardian_reports").insert({user_id:user.id,app_version:String(body.appVersion||"").slice(0,40),android_version:String(body.androidVersion||"").slice(0,40),battery_percent:Number.isFinite(Number(body.batteryPercent))?Math.max(0,Math.min(100,Math.trunc(Number(body.batteryPercent)))):null,available_ram_mb:Number.isFinite(Number(body.availableRamMb))?Math.max(0,Math.min(1000000,Math.trunc(Number(body.availableRamMb)))):null,free_storage_mb:Number.isFinite(Number(body.freeStorageMb))?Math.max(0,Math.min(1000000000,Math.trunc(Number(body.freeStorageMb)))):null,network_type:String(body.network||"").slice(0,30),crash_category:String(body.crashCategory||"").slice(0,64)});
    if(error)throw error;
    return json(200,{recorded:true});
  }
  return fail(400,"Unknown telemetry action");
 }catch(error){
  console.error("CenAI telemetry error",error instanceof Error?error.message:"unknown");
  return fail(500,"Telemetry request failed");
 }
});
