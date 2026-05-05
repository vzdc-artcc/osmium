pub const STYLE: &str = r#"
body{margin:0;padding:0;background:#f1f0f6;color:#1f2430;font-family:Roboto,Arial,Helvetica,sans-serif}
.bg{background:linear-gradient(180deg,#f5f4f8 0%,#f1f0f6 100%);padding:24px}
.shell{max-width:640px;margin:0 auto}
.header{background:#500e0e;padding:30px 32px;color:#ededf5;border-radius:18px 18px 0 0}
.brand{font-size:30px;line-height:1.1;font-weight:700;letter-spacing:.02em}
.eyebrow{font-size:12px;letter-spacing:.16em;text-transform:uppercase;opacity:.88;margin-top:8px}
.panel{background:#ffffff;padding:36px 32px;border-left:1px solid #d9dce5;border-right:1px solid #d9dce5}
.panel h1{margin:0 0 18px;color:#500e0e;font-size:30px;line-height:1.2;font-weight:700}
.panel p{font-size:16px;line-height:1.6;margin:0 0 16px;color:#1f2430}
.panel a{color:#500e0e}
.panel strong{color:#500e0e}
.callout{background:#f7ecec;border-left:4px solid #500e0e;padding:14px 16px;margin:18px 0;border-radius:6px}
.callout p:last-child{margin-bottom:0}
.button{display:inline-block;padding:12px 18px;background:#500e0e;color:#ededf5 !important;text-decoration:none;border-radius:8px;font-weight:700}
.footer{background:#f7f8fb;padding:20px 32px;border:1px solid #d9dce5;border-top:0;border-radius:0 0 18px 18px;color:#5d6472}
.footer p{margin:0;font-size:13px;line-height:1.6}
.footer a{color:#500e0e}
.footer-link{margin-top:8px !important}
.preheader{display:none!important;visibility:hidden;opacity:0;color:transparent;height:0;width:0;overflow:hidden}
@media only screen and (max-width:640px){.bg{padding:12px}.header,.panel,.footer{padding-left:22px;padding-right:22px}.panel{padding-top:30px;padding-bottom:30px}.panel h1{font-size:26px}}
"#;
