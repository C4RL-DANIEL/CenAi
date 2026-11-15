import nodemailer from "nodemailer";
const json = (res,status,body)=>res.status(status).json(body);
const esc = (v="")=>String(v).replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
function template(name,status,note){
 const accepted=status==="approved";
 const heading=accepted?"You're in. Welcome to the CenAI beta.":status==="rejected"?"Your CenAI beta application update":"Your CenAI beta access has changed";
 const detail=accepted?"Your application has been approved. You can now access the public beta and share feedback with the team.":status==="rejected"?"Thank you for applying. We aren't able to offer beta access at this time.":"Your CenAI beta access has been revoked. If you believe this is a mistake, contact the team.";
 return `<!doctype html><html><body style="margin:0;background:#080d17;color:#eaf0ff;font-family:Arial,sans-serif;padding:32px 12px"><table role="presentation" width="100%"><tr><td align="center"><table role="presentation" width="100%" style="max-width:560px;background:#101827;border:1px solid #29364c;border-radius:20px"><tr><td style="padding:28px 32px;background:linear-gradient(120deg,#182a49,#111827)"><div style="font-size:12px;letter-spacing:3px;color:#a9baff;font-weight:bold">CENAI · BETA PROGRAM</div><h1 style="font-size:27px;margin:14px 0 0">${heading}</h1></td></tr><tr><td style="padding:30px 32px"><p style="line-height:1.7;color:#bdc9dc">${detail}</p>${note?'<div style="padding:14px;background:#080f1d;border-radius:12px;color:#bdc9dc"><b>Team note</b><p>'+esc(note)+'</p></div>':''}<p style="margin-top:28px;color:#71829b;font-size:11px">CenAI · Intelligence with permission</p></td></tr></table></td></tr></table></body></html>`;
}
export default async function handler(req,res){
 res.setHeader("Cache-Control","no-store");res.setHeader("X-Content-Type-Options","nosniff");
 if(req.method!=="POST"){res.setHeader("Allow","POST");return json(res,405,{error:"Method not allowed"});}
 const base=process.env.SUPABASE_URL,key=process.env.SUPABASE_PUBLISHABLE_KEY,token=(req.headers.authorization||"").replace(/^Bearer\s+/i,"");
 if(!base||!key||!process.env.EMAIL_USER||!process.env.EMAIL_PASS)return json(res,503,{error:"Email delivery is not configured"});
 if(!token)return json(res,401,{error:"Sign in required"});
 try{
  const auth=await fetch(base.replace(/\/$/,"")+"/auth/v1/user",{headers:{apikey:key,Authorization:"Bearer "+token}});
  const user=await auth.json().catch(()=>({}));
  if(!auth.ok||!user.id)return json(res,401,{error:"Invalid session"});
  const input=req.body||{},id=String(input.applicationId||""),status=String(input.status||""),note=String(input.note||"").slice(0,1000);
  if(!/^[0-9a-f-]{36}$/i.test(id)||!["approved","rejected","revoked"].includes(status))return json(res,400,{error:"Invalid notification request"});
  const check=await fetch(base.replace(/\/$/,"")+"/functions/v1/cenai-control-plane",{method:"POST",headers:{apikey:key,Authorization:"Bearer "+token,"Content-Type":"application/json"},body:JSON.stringify({action:"admin_list",status:"all"})});
  const data=await check.json().catch(()=>({}));
  if(!check.ok)return json(res,check.status,{error:data.error||"Administrator authorization failed"});
  const app=(data.applications||[]).find(x=>x.id===id&&x.status===status);
  if(!app||!app.email)return json(res,404,{error:"Reviewed application was not found"});
  const transport=nodemailer.createTransport({host:"smtp.gmail.com",port:465,secure:true,auth:{user:process.env.EMAIL_USER,pass:process.env.EMAIL_PASS}});
  await transport.sendMail({from:`CenAI Beta <${process.env.EMAIL_USER}>`,to:app.email,subject:status==="approved"?"You're accepted into the CenAI beta":status==="rejected"?"CenAI beta application update":"CenAI beta access update",text:templateText(status,note),html:template(app.display_name,status,note)});
  return json(res,200,{ok:true,sent:true});
 }catch(e){console.error("beta notification failed",e?.message||e);return json(res,503,{error:"Could not send beta notification"});}
}
function templateText(status,note){const base=status==="approved"?"Your CenAI beta application has been approved.":status==="rejected"?"Thank you for applying to the CenAI beta. We cannot offer access at this time.":"Your CenAI beta access has been revoked.";return base+(note?"\n\nTeam note: "+note:"");}
