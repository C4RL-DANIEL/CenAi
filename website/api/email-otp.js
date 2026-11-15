import nodemailer from "nodemailer";
import { createHmac, randomInt, timingSafeEqual } from "node:crypto";

const json = (res, status, body) => res.status(status).json(body);
const escapeHtml = (value = "") => String(value).replace(/[&<>"']/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
const hashCode = (userId, email, code) => createHmac("sha256", process.env.EMAIL_OTP_SECRET).update(userId+"|"+email.toLowerCase()+"|"+code).digest("hex");

function emailHtml(code) {
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Your CenAI verification code</title></head>
  <body style="margin:0;background:#080d17;font-family:Arial,Helvetica,sans-serif;color:#eaf0ff;padding:32px 12px">
  <table role="presentation" width="100%" cellspacing="0" cellpadding="0"><tr><td align="center">
  <table role="presentation" width="100%" style="max-width:560px;background:#101827;border:1px solid #29364c;border-radius:20px;overflow:hidden" cellspacing="0" cellpadding="0">
  <tr><td style="padding:28px 32px;background:linear-gradient(120deg,#182a49,#111827)"><div style="font-size:12px;letter-spacing:3px;color:#a9baff;font-weight:bold">CENAI · ACCOUNT SECURITY</div><h1 style="margin:12px 0 0;font-size:27px">Verify it's really you.</h1></td></tr>
  <tr><td style="padding:30px 32px"><p style="color:#bdc9dc;line-height:1.7">Use this one-time code to continue. It expires in <b style="color:#fff">10 minutes</b>.</p>
  <div style="margin:26px 0;padding:20px;background:#080f1d;border:1px solid #34476a;border-radius:14px;text-align:center;font-size:34px;font-weight:800;letter-spacing:9px;color:#a9baff">${code}</div>
  <p style="font-size:13px;color:#91a2bc;line-height:1.7">If you didn't request this code, you can safely ignore this message. Never share your code with anyone, including CenAI support.</p>
  <div style="border-top:1px solid #29364c;margin-top:28px;padding-top:18px;color:#71829b;font-size:11px">CenAI · Intelligence with permission</div></td></tr></table></td></tr></table></body></html>`;
}

export default async function handler(req, res) {
  res.setHeader("Cache-Control", "no-store");
  res.setHeader("X-Content-Type-Options", "nosniff");
  if (req.method !== "POST") { res.setHeader("Allow", "POST"); return json(res,405,{error:"Method not allowed"}); }
  const url = process.env.SUPABASE_URL, key = process.env.SUPABASE_PUBLISHABLE_KEY;
  const userToken = (req.headers.authorization || "").replace(/^Bearer\s+/i, "");
  if (!url || !key || !process.env.EMAIL_USER || !process.env.EMAIL_PASS || !process.env.EMAIL_OTP_SECRET) return json(res,503,{error:"Email verification is not fully configured"});
  if (!userToken) return json(res,401,{error:"Sign in required"});
  try {
    const userResponse = await fetch(url.replace(/\/$/,"")+"/auth/v1/user", {headers:{apikey:key,Authorization:"Bearer "+userToken}});
    const user = await userResponse.json().catch(()=>({}));
    if (!userResponse.ok || !user.id || !user.email) return json(res,401,{error:"Valid signed-in account required"});
    const action = String(req.body?.action || "");
    const email = String(req.body?.email || user.email).trim().toLowerCase();
    if (email !== String(user.email).toLowerCase()) return json(res,403,{error:"You can only verify the email on your signed-in account"});
    if (action === "send") {
      const code = String(randomInt(0,1000000)).padStart(6,"0");
      const digest = hashCode(user.id,email,code);
      const rpc = await fetch(url.replace(/\/$/,"")+"/rest/v1/rpc/cenai_issue_email_otp", {
        method:"POST",headers:{apikey:key,Authorization:"Bearer "+userToken,"Content-Type":"application/json"},
        body:JSON.stringify({p_email:email,p_otp_hash:digest})
      });
      if (!rpc.ok) { const e=await rpc.json().catch(()=>({})); return json(res,rpc.status,{error:e.message||"Could not create verification challenge"}); }
      const transport = nodemailer.createTransport({host:"smtp.gmail.com",port:465,secure:true,auth:{user:process.env.EMAIL_USER,pass:process.env.EMAIL_PASS}});
      await transport.sendMail({from:`CenAI Security <${process.env.EMAIL_USER}>`,to:email,subject:"Your CenAI verification code",text:`Your CenAI verification code is ${code}. It expires in 10 minutes. Do not share it.`,html:emailHtml(code)});
      return json(res,200,{ok:true,message:"Verification code sent. Check your inbox."});
    }
    if (action === "verify") {
      const code = String(req.body?.code || "").trim();
      if (!/^\d{6}$/.test(code)) return json(res,400,{error:"Enter the six-digit verification code"});
      const digest = hashCode(user.id,email,code);
      const rpc = await fetch(url.replace(/\/$/,"")+"/rest/v1/rpc/cenai_verify_email_otp", {
        method:"POST",headers:{apikey:key,Authorization:"Bearer "+userToken,"Content-Type":"application/json"},
        body:JSON.stringify({p_email:email,p_otp_hash:digest})
      });
      const ok = rpc.ok ? await rpc.json() : false;
      if (!ok) return json(res,400,{error:"Code is invalid, expired, or already used"});
      return json(res,200,{ok:true,verified:true,message:"Email verified for this CenAI account"});
    }
    return json(res,400,{error:"Unknown email verification action"});
  } catch (error) {
    console.error("CenAI email verification failed", error?.message || error);
    return json(res,503,{error:"Email verification service temporarily unavailable"});
  }
}