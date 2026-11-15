import "jsr:@supabase/functions-js/edge-runtime.d.ts";
import { createClient } from "npm:@supabase/supabase-js@2.57.4";

const OWNER_EMAIL = "aglibotcarldaniel@gmail.com";
const RESET_SECRET_SHA256 = "154232137d23461257c406d6e8ade32eb6e111e356fd32af84a287d18d157283";
const adminDb = createClient(Deno.env.get("SUPABASE_URL")!, Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!, { auth:{persistSession:false,autoRefreshToken:false} });
const json=(status:number,body:unknown)=>new Response(JSON.stringify(body),{status,headers:{"Content-Type":"application/json","Cache-Control":"no-store","X-Content-Type-Options":"nosniff"}});
const fail=(status:number,error:string)=>json(status,{error});
async function sha256(value:string){const digest=await crypto.subtle.digest("SHA-256",new TextEncoder().encode(value));return Array.from(new Uint8Array(digest)).map(x=>x.toString(16).padStart(2,"0")).join("");}
function safeEqual(a:string,b:string){if(a.length!==b.length)return false;let n=0;for(let i=0;i<a.length;i++)n|=a.charCodeAt(i)^b.charCodeAt(i);return n===0;}
async function ownerIdentity(){
 const {data:admin,error}=await adminDb.from("admin_users").select("user_id").eq("role","owner").eq("is_active",true).limit(1).maybeSingle();
 if(error)throw error;
 if(!admin?.user_id)throw new Error("Active owner account not found");
 const {data,error:userError}=await adminDb.auth.admin.getUserById(admin.user_id);
 if(userError)throw userError;
 if(!data.user||String(data.user.email||"").toLowerCase()!==OWNER_EMAIL)throw new Error("Owner identity verification failed");
 return {id:admin.user_id as string,user:data.user};
}
async function authorize(req:Request){
 const supplied=req.headers.get("x-cenai-reset-secret")||"";
 if(supplied.length<32)return false;
 return safeEqual(await sha256(supplied),RESET_SECRET_SHA256);
}
Deno.serve(async(req:Request)=>{
 if(req.method!=="POST")return fail(405,"POST required");
 if(req.headers.has("origin"))return fail(403,"Server-to-server requests only");
 try{
  if(!await authorize(req))return fail(401,"Unauthorized reset request");
  const body=await req.json().catch(()=>({}));
  const action=String(body.action||"");
  const {id:ownerId,user:owner}=await ownerIdentity();
  if(action==="issue"){
    const otpHash=String(body.otpHash||"").toLowerCase();
    if(!/^[a-f0-9]{64}$/.test(otpHash))return fail(400,"Invalid verification challenge");
    const {data:previous,error:previousError}=await adminDb.from("email_otp_challenges").select("created_at").eq("user_id",ownerId).maybeSingle();
    if(previousError)throw previousError;
    if(previous?.created_at&&new Date(previous.created_at).getTime()>Date.now()-60_000)return fail(429,"Please wait before requesting another reset code");
    const {error}=await adminDb.from("email_otp_challenges").upsert({user_id:ownerId,email:OWNER_EMAIL,otp_hash:otpHash,expires_at:new Date(Date.now()+10*60_000).toISOString(),attempts:0,consumed_at:null,created_at:new Date().toISOString()});
    if(error)throw error;
    return json(200,{ok:true});
  }
  if(action==="cancel"){
    const {error}=await adminDb.from("email_otp_challenges").delete().eq("user_id",ownerId).eq("email",OWNER_EMAIL);
    if(error)throw error;
    return json(200,{ok:true});
  }
  if(action==="complete"){
    const otpHash=String(body.otpHash||"").toLowerCase();
    const newPassword=String(body.newPassword||"");
    if(!/^[a-f0-9]{64}$/.test(otpHash))return fail(400,"Enter the valid email verification code");
    if(newPassword.length<12||newPassword.length>128)return fail(400,"Use a password between 12 and 128 characters");
    const {data:challenge,error:challengeError}=await adminDb.from("email_otp_challenges").select("otp_hash,expires_at,attempts,consumed_at").eq("user_id",ownerId).eq("email",OWNER_EMAIL).maybeSingle();
    if(challengeError)throw challengeError;
    if(!challenge||challenge.consumed_at||!challenge.expires_at||new Date(challenge.expires_at).getTime()<=Date.now()||Number(challenge.attempts)>=5)return fail(400,"The code is invalid, expired or already used");
    const {data:claimed,error:claimError}=await adminDb.from("email_otp_challenges").update({attempts:Number(challenge.attempts)+1}).eq("user_id",ownerId).eq("attempts",challenge.attempts).is("consumed_at",null).select("otp_hash").maybeSingle();
    if(claimError)throw claimError;
    if(!claimed)return fail(409,"A reset attempt is already being processed");
    if(!safeEqual(String(claimed.otp_hash||""),otpHash))return fail(400,"The code is invalid, expired or already used");
    const {error:updateError}=await adminDb.auth.admin.updateUserById(ownerId,{password:newPassword,user_metadata:{...(owner.user_metadata||{}),username:"CenAI",name:"CenAI"}});
    if(updateError)throw updateError;
    const {error:consumeError}=await adminDb.from("email_otp_challenges").update({consumed_at:new Date().toISOString(),otp_hash:""}).eq("user_id",ownerId);
    if(consumeError)throw consumeError;
    const {error:auditError}=await adminDb.from("admin_audit_logs").insert({actor_user_id:ownerId,actor_email:OWNER_EMAIL,action:"admin.password.reset",details:{method:"gmail_email_otp",username:"CenAI"}});
    if(auditError)throw auditError;
    return json(200,{ok:true,updated:true});
  }
  return fail(400,"Unknown reset action");
 }catch(error){
  console.error("administrator password reset failed",error instanceof Error?error.message:"unknown");
  return fail(500,"Administrator password reset is temporarily unavailable");
 }
});
