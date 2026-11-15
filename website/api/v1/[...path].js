export default async function handler(req,res){
  res.setHeader("Cache-Control","no-store");
  res.setHeader("X-Content-Type-Options","nosniff");
  res.setHeader("Vary","X-API-Key,Authorization");
  if(req.method!=="GET"){res.setHeader("Allow","GET");return res.status(405).json({error:"Only GET is supported by this API version"});}
  const key=(req.headers["x-api-key"]||"").toString() || (req.headers.authorization||"").toString().replace(/^Bearer\s+/i,"");
  if(!key)return res.status(401).json({error:"A CenAI API key is required"});
  const path=Array.isArray(req.query.path)?req.query.path.join("/"):(req.query.path||"").toString();
  if(!["models","usage"].includes(path))return res.status(404).json({error:"Unknown API endpoint",available:["GET /api/v1/models","GET /api/v1/usage"]});
  const base=process.env.SUPABASE_URL;
  if(!base)return res.status(503).json({error:"CenAI API is not configured"});
  try{
    const response=await fetch(base.replace(/\/$/,"")+"/functions/v1/cenai-public-api/v1/"+path,{headers:{"X-API-Key":key,"Accept":"application/json"}});
    const body=await response.json().catch(()=>({error:"Invalid API response"}));
    return res.status(response.status).json(body);
  }catch(error){
    return res.status(502).json({error:"CenAI API gateway is temporarily unavailable"});
  }
}
