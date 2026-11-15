const jsonHeaders={"Content-Type":"application/json","apikey":process.env.SUPABASE_SECRET_KEY||process.env.SUPABASE_SERVICE_ROLE_KEY||""};
async function db(path,method="GET",body){const base=process.env.SUPABASE_URL;if(!base||!jsonHeaders.apikey)throw new Error("Supabase server integration is not configured");const r=await fetch(base.replace(/\\/$/,"")+"/rest/v1/"+path,{method,headers:{...jsonHeaders,Authorization:"Bearer "+jsonHeaders.apikey,Prefer:"return=representation"},...(body?{body:JSON.stringify(body)}:{})});const data=await r.json().catch(()=>null);if(!r.ok)throw new Error("Database request failed ("+r.status+")");return data;}
async function identity(req){const token=(req.headers.authorization||"").replace(/^Bearer\\s+/i,"");if(!token)throw new Error("Sign in required");const base=process.env.SUPABASE_URL,key=process.env.SUPABASE_PUBLISHABLE_KEY;if(!base||!key)throw new Error("Supabase authentication is not configured");const r=await fetch(base.replace(/\\/$/,"")+"/auth/v1/user",{headers:{apikey:key,Authorization:"Bearer "+token}});if(!r.ok)throw new Error("Invalid or expired session");return r.json();}
async function admin(user){const rows=await db("admin_users?user_id=eq."+encodeURIComponent(user.id)+"&is_active=eq.true&select=role");if(!rows?.length)throw new Error("Administrator access denied");return rows[0].role;}
async function audit(user,action,details){await db("admin_audit_logs","POST",{actor_user_id:user.id,actor_email:user.email||"",action,details});}
function reply(res,status,data){return res.status(status).json(data);}
export default async function handler(req,res){
 res.setHeader("Cache-Control","no-store");res.setHeader("X-Content-Type-Options","nosniff");res.setHeader("Vary","Authorization");
 if(req.method!=="POST"){res.setHeader("Allow","POST");return reply(res,405,{error:"Method not allowed"});}
 try{
  const user=await identity(req),body=typeof req.body==="string"?JSON.parse(req.body):req.body||{},action=body.action;
  if(action==="apply"){
   const flags=await db("admin_feature_flags?key=in.(beta.applications.open,beta.invite_only)&select=key,enabled");
   if(flags.find(x=>x.key==="beta.applications.open")?.enabled!==true)return reply(res,403,{error:"Beta applications are currently closed"});
   if(flags.find(x=>x.key==="beta.invite_only")?.enabled===true&&!body.inviteCode)return reply(res,403,{error:"An invitation is required"});
   const displayName=String(body.displayName||"").trim(),motivation=String(body.motivation||"").trim(),experience=String(body.experience||"new");
   if(displayName.length<1||displayName.length>80||motivation.length<20||motivation.length>2000||!["new","some","advanced"].includes(experience))return reply(res,400,{error:"Please provide a display name and a 20–2000 character application note."});
   const existing=await db("beta_applications?user_id=eq."+encodeURIComponent(user.id)+"&select=id,status");
   if(existing.length)return reply(res,409,{error:"You already have a beta application. Check its status in your beta dashboard.",application:existing[0]});
   const rows=await db("beta_applications","POST",{user_id:user.id,email:user.email||"",display_name:displayName,motivation,experience});
   return reply(res,201,{application:rows[0]});
  }
  if(action==="status"){
   const rows=await db("beta_applications?user_id=eq."+encodeURIComponent(user.id)+"&select=id,display_name,status,reviewer_note,created_at,reviewed_at");
   return reply(res,200,{application:rows[0]||null,isBetaTester:rows[0]?.status==="approved"});
  }
  if(action==="feedback"){
   const approved=await db("beta_applications?user_id=eq."+encodeURIComponent(user.id)+"&status=eq.approved&select=id");
   if(!approved.length)return reply(res,403,{error:"Only approved beta testers can submit feedback"});
   const category=String(body.category||"other"),title=String(body.title||"").trim(),content=String(body.body||"").trim();
   if(!["bug","idea","ux","performance","other"].includes(category)||title.length<4||title.length>120||content.length<10||content.length>5000)return reply(res,400,{error:"Invalid feedback. Check the title and description lengths."});
   const rows=await db("beta_feedback","POST",{user_id:user.id,category,title,body:content,app_version:String(body.appVersion||"").slice(0,40)});
   return reply(res,201,{feedback:rows[0]});
  }
  const role=await admin(user);
  if(action==="admin_list"){
   const status=String(body.status||"pending");
   if(!["pending","approved","rejected","revoked","all"].includes(status))return reply(res,400,{error:"Invalid status filter"});
   const filter=status==="all"?"":"&status=eq."+status;
   const rows=await db("beta_applications?select=id,user_id,email,display_name,motivation,experience,status,reviewer_note,created_at,reviewed_at&order=created_at.desc&limit=200"+filter);
   return reply(res,200,{applications:rows});
  }
  if(action==="admin_review"){
   if(!["owner","admin"].includes(role))return reply(res,403,{error:"Owner or admin role required"});
   const id=String(body.id||""),status=String(body.status||""),note=String(body.note||"").trim();
   if(!/^[0-9a-f-]{36}$/i.test(id)||!["approved","rejected","revoked"].includes(status)||note.length>1000)return reply(res,400,{error:"Invalid review request"});
   const rows=await db("beta_applications?id=eq."+encodeURIComponent(id),"PATCH",{status,reviewer_id:user.id,reviewer_note:note,reviewed_at:new Date().toISOString(),updated_at:new Date().toISOString()});
   if(!rows.length)return reply(res,404,{error:"Application not found"});
   await audit(user,"beta.application."+status,{application_id:id,target_user_id:rows[0].user_id});
   return reply(res,200,{application:rows[0]});
  }
  if(action==="admin_feedback"){
   const rows=await db("beta_feedback?select=id,user_id,category,title,body,app_version,created_at&order=created_at.desc&limit=200");
   return reply(res,200,{feedback:rows});
  }
  return reply(res,400,{error:"Unknown beta action"});
 }catch(e){const denied=/required|denied|expired|invalid/i.test(e.message);return reply(res,denied?401:503,{error:e.message||"Beta service unavailable"});}
}