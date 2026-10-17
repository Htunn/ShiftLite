import { Navigate, Route, Routes, useParams } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import Navbar from './components/common/Navbar';
import Sidebar from './components/common/Sidebar';
import Login from './pages/Login';
import Overview from './pages/Overview';
import ApplicationDetail from './pages/ApplicationDetail';

async function fetchMe() {
  const res = await fetch('/api/v1/auth/me', { credentials: 'include' });
  if (res.status === 401) throw new Error('unauthenticated');
  if (!res.ok) throw new Error(`me request failed: ${res.status}`);
  return res.json();
}

function ProtectedLayout({ children }: { children: React.ReactNode }) {
  const { isLoading, error } = useQuery({ queryKey: ['me'], queryFn: fetchMe, retry: false });

  // Any 401 from the session check redirects to /login (SPEC-001 UI notes).
  if (error) return <Navigate to="/login" replace />;
  if (isLoading) return <p className="p-6 text-slate-400">Loading…</p>;

  return (
    <div className="flex min-h-screen flex-col">
      <Navbar />
      <div className="flex flex-1">
        <Sidebar />
        <main className="flex-1 p-6">{children}</main>
      </div>
    </div>
  );
}

function ApplicationDetailRoute() {
  useParams<{ namespace: string; appName: string }>();
  return <ApplicationDetail />;
}

export default function App() {
  return (
    <Routes>
      <Route path="/login" element={<Login />} />
      <Route
        path="/"
        element={
          <ProtectedLayout>
            <Overview />
          </ProtectedLayout>
        }
      />
      <Route
        path="/namespaces/:namespace/apps/:appName"
        element={
          <ProtectedLayout>
            <ApplicationDetailRoute />
          </ProtectedLayout>
        }
      />
    </Routes>
  );
}
