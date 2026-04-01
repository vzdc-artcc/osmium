use axum::{response::Html, Json};
use serde_json::json;

/// Get API documentation in HTML format
pub async fn get_docs_html() -> Html<&'static str> {
    Html(include_str!("../../docs/api.html"))
}

/// Get OpenAPI schema in JSON format
pub async fn get_openapi_schema() -> Json<serde_json::Value> {
    // This is a simplified schema - in production, generate this from utoipa
    Json(json!({
        "openapi": "3.0.0",
        "info": {
            "title": "Osmium API",
            "version": "1.0.0",
            "description": "ARTCC management and training platform API"
        },
        "servers": [
            {
                "url": "http://localhost:3000",
                "description": "Local development server"
            }
        ],
        "paths": {
            "/api/v1/me": {
                "get": {
                    "summary": "Get current user",
                    "tags": ["auth"],
                    "responses": {
                        "200": {
                            "description": "Current user info"
                        },
                        "401": {
                            "description": "Not authenticated"
                        }
                    }
                }
            },
            "/api/v1/auth/vatsim/login": {
                "get": {
                    "summary": "VATSIM login redirect",
                    "tags": ["auth"],
                    "responses": {
                        "307": {
                            "description": "Redirect to VATSIM"
                        }
                    }
                }
            },
            "/api/v1/auth/logout": {
                "post": {
                    "summary": "Logout user",
                    "tags": ["auth"],
                    "responses": {
                        "200": {
                            "description": "Logout successful"
                        }
                    }
                }
            },
            "/api/v1/user": {
                "get": {
                    "summary": "List users",
                    "tags": ["users"],
                    "parameters": [
                        {
                            "name": "limit",
                            "in": "query",
                            "description": "Results limit",
                            "schema": { "type": "integer", "default": 50 }
                        },
                        {
                            "name": "offset",
                            "in": "query",
                            "description": "Results offset",
                            "schema": { "type": "integer", "default": 0 }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of users"
                        }
                    }
                }
            },
            "/api/v1/stats/artcc": {
                "get": {
                    "summary": "Get ARTCC statistics",
                    "tags": ["stats"],
                    "responses": {
                        "200": {
                            "description": "ARTCC statistics"
                        }
                    }
                }
            },
            "/api/v1/files": {
                "get": {
                    "summary": "List file assets",
                    "tags": ["files"]
                },
                "post": {
                    "summary": "Upload a file asset",
                    "tags": ["files"]
                }
            },
            "/api/v1/files/{file_id}": {
                "get": {
                    "summary": "Get file metadata",
                    "tags": ["files"]
                },
                "patch": {
                    "summary": "Update file metadata",
                    "tags": ["files"]
                },
                "delete": {
                    "summary": "Delete file",
                    "tags": ["files"]
                }
            },
            "/api/v1/files/{file_id}/content": {
                "get": {
                    "summary": "Download file content",
                    "tags": ["files"]
                },
                "put": {
                    "summary": "Replace file content",
                    "tags": ["files"]
                }
            },
            "/api/v1/files/{file_id}/signed-url": {
                "get": {
                    "summary": "Get signed CDN URL",
                    "tags": ["files"]
                }
            },
            "/cdn/{file_id}": {
                "get": {
                    "summary": "CDN download (public or signed token)",
                    "tags": ["files"]
                }
            }
        }
    }))
}

/// Health check endpoint
pub async fn docs_health() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "message": "Documentation service is running"
    }))
}
