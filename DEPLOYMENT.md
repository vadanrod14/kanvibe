# Deployment Guide for Vibe Kanban

## Overview
This app has been configured for deployment to Google Cloud Platform (GCP) with the project ID `grouplang-450317`:
- **Frontend**: Firebase Hosting (or Google Cloud Storage as static site)
- **Backend**: Google Cloud Run

## Prerequisites
1. Google Cloud CLI (`gcloud`) installed and authenticated
2. Firebase CLI installed and authenticated
3. Docker installed (for building container images)
4. Access to the GCP project `grouplang-450317`

## Frontend Deployment

### Option 1: Firebase Hosting

The frontend is configured for Firebase Hosting. Files have been created:
- `firebase.json` - Firebase configuration
- `.firebaserc` - Project configuration
- `frontend/.env.production` - Production environment variables

**Steps to deploy:**

1. **Initialize Firebase (if not done):**
   ```bash
   firebase login
   firebase init hosting --project grouplang-450317
   # Select "frontend/dist" as public directory
   # Configure as SPA (rewrite all URLs to /index.html): Yes
   # Overwrite index.html: No
   ```

2. **Build and deploy:**
   ```bash
   # Build frontend
   cd frontend && npm run build
   
   # Deploy to Firebase
   firebase deploy --only hosting --project grouplang-450317
   ```

### Option 2: Google Cloud Storage (Alternative)

If Firebase Hosting doesn't work, you can use GCS:

1. **Create a bucket:**
   ```bash
   gsutil mb -p grouplang-450317 gs://vibe-kanban-frontend
   gsutil web set -m index.html -e index.html gs://vibe-kanban-frontend
   gsutil iam ch allUsers:objectViewer gs://vibe-kanban-frontend
   ```

2. **Upload files:**
   ```bash
   cd frontend
   npm run build
   gsutil -m rsync -r -d dist/ gs://vibe-kanban-frontend
   ```

## Backend Deployment

The backend is configured for Google Cloud Run with:
- `Dockerfile` - Container configuration
- `cloudbuild.yaml` - Build and deployment configuration
- `.dockerignore` - Files to exclude from build

**Steps to deploy:**

1. **Enable required APIs:**
   ```bash
   gcloud services enable cloudbuild.googleapis.com run.googleapis.com --project grouplang-450317
   ```

2. **Build and deploy using Cloud Build:**
   ```bash
   gcloud builds submit --config cloudbuild.yaml --project grouplang-450317
   ```

   OR manually with Docker:

   ```bash
   # Build image
   docker build -t gcr.io/grouplang-450317/vibe-kanban-backend .
   
   # Push to registry
   docker push gcr.io/grouplang-450317/vibe-kanban-backend
   
   # Deploy to Cloud Run
   gcloud run deploy vibe-kanban-backend \
     --image gcr.io/grouplang-450317/vibe-kanban-backend \
     --region us-central1 \
     --platform managed \
     --allow-unauthenticated \
     --port 8080 \
     --memory 1Gi \
     --cpu 1 \
     --project grouplang-450317
   ```

## Configuration Updates

### Frontend API Configuration
The frontend is configured to use the production API URL in `frontend/.env.production`:
```
VITE_API_URL=https://vibe-kanban-backend-us-central1.run.app
```

Update this URL after deploying the backend to match your actual Cloud Run service URL.

### Backend Environment Variables
The backend will need these environment variables in production:
- `DATABASE_URL`: SQLite database path (configured in Dockerfile)
- `PORT`: 8080 (configured in Dockerfile)
- `RUST_LOG`: info (configured in Dockerfile)

For external services, you may need to set:
- GitHub OAuth credentials
- Sentry DSN
- Other API keys

You can set these in Cloud Run:
```bash
gcloud run services update vibe-kanban-backend \
  --set-env-vars "GITHUB_CLIENT_ID=your_id,GITHUB_CLIENT_SECRET=your_secret" \
  --region us-central1 \
  --project grouplang-450317
```

## Persistent Storage

The current configuration uses SQLite with a local file. For production, consider:

1. **Cloud SQL** for better scalability and reliability
2. **Persistent disk** mounted to Cloud Run (if sticking with SQLite)

## Security Considerations

1. **HTTPS**: Both Firebase Hosting and Cloud Run provide HTTPS by default
2. **CORS**: Update backend CORS settings to allow your frontend domain
3. **Authentication**: Ensure GitHub OAuth is configured with correct redirect URLs
4. **API Keys**: Store sensitive keys in Google Secret Manager

## Monitoring

Consider setting up:
1. **Cloud Monitoring** for backend metrics
2. **Cloud Logging** for application logs
3. **Error Reporting** for crash analysis

## Domain Configuration

After deployment:
1. Frontend will be available at: `https://grouplang-450317.web.app` (Firebase) or your custom domain
2. Backend will be available at: `https://vibe-kanban-backend-us-central1.run.app`

To use a custom domain:
- For frontend: Configure Firebase Hosting custom domain
- For backend: Configure Cloud Run custom domain mapping

## Troubleshooting

1. **Build failures**: Check `cloudbuild.yaml` and `Dockerfile` for any missing dependencies
2. **Permission errors**: Ensure your account has necessary IAM roles for the project
3. **API errors**: Verify all required APIs are enabled
4. **CORS issues**: Update backend CORS configuration for your frontend domain

## Known Issues

### Rust Edition 2024 Dependency Conflict

The current backend has a dependency chain that requires Rust Edition 2024 support, which is not yet stable in the current Rust toolchain. This affects the `base64ct` and related cryptographic dependencies.

**Temporary Solutions:**
1. Use Rust nightly toolchain: `FROM rustlang/rust:nightly-slim`
2. Remove optional dependencies that require Edition 2024
3. Pin dependency versions to older releases

**Long-term Solution:**
Wait for Rust Edition 2024 to stabilize in the main Rust toolchain (expected early 2025), then update the Dockerfile to use the latest stable Rust version.

### Current Status

✅ **Frontend Deployed Successfully**
- Location: Google Cloud Storage 
- URL: https://storage.googleapis.com/vibe-kanban-frontend-2025/index.html
- Status: Fully functional static site

❌ **Backend Deployment Pending**
- Issue: Rust Edition 2024 dependency conflict
- Infrastructure: Ready (Cloud Run, Artifact Registry configured)
- Solution: Requires dependency update or nightly Rust toolchain

## Files Created/Modified

- `Dockerfile` - Backend container configuration
- `cloudbuild.yaml` - GCP build configuration
- `.dockerignore` - Docker build exclusions
- `firebase.json` - Firebase hosting configuration
- `.firebaserc` - Firebase project configuration
- `frontend/.env.production` - Frontend production environment
- `frontend/src/lib/api.ts` - Updated API base URL configuration

All configuration files are ready for deployment to the `grouplang-450317` GCP project.