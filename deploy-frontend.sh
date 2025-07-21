#!/bin/bash

echo "🚀 Deploying Vibe Kanban Frontend to Firebase Hosting"
echo "=================================================="

# Build the frontend
echo "📦 Building frontend..."
cd frontend
npm run build 2>/dev/null || npx vite build

# Check if firebase.json exists
if [ ! -f "../firebase.json" ]; then
    echo "❌ firebase.json not found. Please run 'firebase init hosting' first."
    exit 1
fi

# Deploy to Firebase Hosting
echo "🌐 Deploying to Firebase..."
cd ..
echo "Run the following command to deploy:"
echo "firebase deploy --only hosting --project grouplang-450317"
echo ""
echo "📍 Frontend will be available at:"
echo "https://grouplang-450317.web.app"
echo ""
echo "Note: You need to be authenticated with Firebase CLI first:"
echo "firebase login"