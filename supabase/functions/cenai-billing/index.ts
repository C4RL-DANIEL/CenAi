import "jsr:@supabase/functions-js/edge-runtime.d.ts";
import { createClient } from "npm:@supabase/supabase-js@2.57.4";

const url=Deno.env.get("SUPABASE_URL")!;
const anon=Deno.env.get("SUPABASE_ANON_KEY")!;
const service=Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!;
const db=createClient(url,service,{auth:{persistSession:false,autoRefreshToken:false}});
const authDb=createClient(url,anon,{auth:{persistSession:false,autoRefreshToken:false}});
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

function hex(bytes:Uint8Array){return Array.from(bytes).map(x=>x.toString(16).padStart(2,"0")).join("");}
function safeEqual(a:string,b:string){if(a.length!==b.length)return false;let n=0;for(let i=0;i<a.length;i++)n|=a.charCodeAt(i)^b.charCodeAt(i);return n===0;}
async function currentUser(req:Request){
 const token=(req.headers.get("Authorization")||"").replace(/^Bearer\s+/i,"");
 if(!token)return null;
 const {data,error}=await authDb.auth.getUser(token);
 if(error||!data.user||data.user.is_anonymous)return null;
 return data.user;
}
async function verifyStripe(raw:string,header:string,secret:string){
 const parts=Object.fromEntries(header.split(",").map(x=>x.split("=",2)));
 const timestamp=parts.t||"",signature=parts.v1||"";
 if(!/^\d+$/.test(timestamp)||!signature)return false;
 if(Math.abs(Date.now()/1000-Number(timestamp))>300)return false;
 const key=await crypto.subtle.importKey("raw",new TextEncoder().encode(secret),{name:"HMAC",hash:"SHA-256"},false,["sign"]);
 const signed=await crypto.subtle.sign("HMAC",key,new TextEncoder().encode(timestamp+"."+raw));
 return safeEqual(hex(new Uint8Array(signed)),signature);
}
async function stripePost(path:string,params:URLSearchParams){
 const key=Deno.env.get("STRIPE_SECRET_KEY");
 if(!key)throw new Error("Stripe is not configured");
 const response=await fetch("https://api.stripe.com/v1/"+path,{method:"POST",headers:{"Authorization":"Bearer "+key,"Content-Type":"application/x-www-form-urlencoded"},body:params});
 const data=await response.json().catch(()=>({}));
 if(!response.ok)throw new Error("Stripe request failed ("+response.status+")");
 return data;
}
async function handleWebhook(event:any){
 const object=event.data?.object||{};
 if(event.type==="checkout.session.completed"){
   const userId=String(object.client_reference_id||object.metadata?.user_id||"");
   if(userId&&object.customer){
     const {error}=await db.from("billing_customers").upsert({user_id:userId,provider:"stripe",provider_customer_id:String(object.customer),email:String(object.customer_details?.email||object.customer_email||"")},{onConflict:"user_id"});
     if(error)throw error;
   }
 }
 if(["customer.subscription.created","customer.subscription.updated","customer.subscription.deleted"].includes(event.type)){
   const customerId=String(object.customer||"");
   let userId=String(object.metadata?.user_id||"");
   if(!userId&&customerId){const {data}=await db.from("billing_customers").select("user_id").eq("provider_customer_id",customerId).maybeSingle();userId=data?.user_id||"";}
   if(userId){
     const {error}=await db.from("billing_subscriptions").upsert({user_id:userId,provider:"stripe",provider_subscription_id:String(object.id),plan_key:String(object.metadata?.plan_key||"supporter"),status:String(object.status||"unknown"),current_period_end:object.current_period_end?new Date(object.current_period_end*1000).toISOString():null,cancel_at_period_end:object.cancel_at_period_end===true,updated_at:new Date().toISOString()},{onConflict:"provider_subscription_id"});
     if(error)throw error;
   }
 }
 if(event.type==="invoice.payment_failed"&&object.subscription){
   const {error}=await db.from("billing_subscriptions").update({status:"past_due",updated_at:new Date().toISOString()}).eq("provider_subscription_id",String(object.subscription));
   if(error)throw error;
 }
}
Deno.serve(async(req:Request)=>{
 if(req.method!=="POST")return fail(405,"POST required");
 try{
   const stripeKey=Deno.env.get("STRIPE_SECRET_KEY")||"";
   const webhookSecret=Deno.env.get("STRIPE_WEBHOOK_SECRET")||"";
   const publicUrl="https://cenai-website.aglibotcarldaniel.workers.dev";
   const stripeSignature=req.headers.get("Stripe-Signature")||"";
   if(stripeSignature){
     if(!webhookSecret)return fail(503,"Stripe webhook is not configured");
     const raw=await req.text();
     if(!await verifyStripe(raw,stripeSignature,webhookSecret))return fail(400,"Invalid Stripe webhook signature");
     const event=JSON.parse(raw);
     const {data:existing,error:lookupError}=await db.from("billing_webhook_events").select("event_id").eq("provider","stripe").eq("event_id",String(event.id)).maybeSingle();
     if(lookupError)throw lookupError;
     if(existing)return json(200,{received:true,duplicate:true});
     await handleWebhook(event);
     const {error:insertError}=await db.from("billing_webhook_events").insert({provider:"stripe",event_id:String(event.id)});
     if(insertError && insertError.code!=="23505")throw insertError;
     return json(200,{received:true});
   }
   if(!stripeKey||!webhookSecret||!publicUrl)return fail(503,"Billing is not configured. The administrator must configure Stripe server secrets and the public site URL.");
   if(!(await db.from("admin_feature_flags").select("enabled").eq("key","billing.enabled").maybeSingle()).data?.enabled) return fail(503,"Billing is disabled by the platform owner.");
   const user=await currentUser(req);
   if(!user)return fail(401,"Sign in required");
   const body=await req.json().catch(()=>({}));
   const plan=String(body.plan||"");
   const price=plan==="supporter"?Deno.env.get("STRIPE_PRICE_SUPPORTER"):plan==="pro"?Deno.env.get("STRIPE_PRICE_PRO"):null;
   if(!price)return fail(400,"Unknown plan or missing Stripe price configuration");
   const {data:customer}=await db.from("billing_customers").select("provider_customer_id").eq("user_id",user.id).maybeSingle();
   const params=new URLSearchParams();
   params.set("mode","subscription");
   params.set("line_items[0][price]",price);
   params.set("line_items[0][quantity]","1");
   params.set("success_url",publicUrl.replace(/\/$/,"")+"/billing/success?session_id={CHECKOUT_SESSION_ID}");
   params.set("cancel_url",publicUrl.replace(/\/$/,"")+"/billing/cancel");
   params.set("client_reference_id",user.id);
   params.set("metadata[user_id]",user.id);
   params.set("metadata[plan_key]",plan);
   params.set("subscription_data[metadata][user_id]",user.id);
   params.set("subscription_data[metadata][plan_key]",plan);
   if(customer?.provider_customer_id)params.set("customer",customer.provider_customer_id);
   else if(user.email)params.set("customer_email",user.email);
   const session=await stripePost("checkout/sessions",params);
   if(!session.url)return fail(502,"Stripe did not return a checkout URL");
   return json(200,{checkoutUrl:session.url,sessionId:session.id});
 }catch(error){
   console.error("CenAI billing error",error instanceof Error?error.message:"unknown");
   return fail(500,"Billing request failed");
 }
});
