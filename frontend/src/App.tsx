import { Route, Routes } from "react-router-dom";
import { Layout } from "./components/Layout";
import { ToastHost } from "./components/ui";
import Overview from "./pages/Overview";
import Library from "./pages/Library";
import Compression from "./pages/Compression";
import Comparison from "./pages/Comparison";
import Inference from "./pages/Inference";
import Monitor from "./pages/Monitor";
import History from "./pages/History";
import SettingsPage from "./pages/Settings";

export default function App() {
  return (
    <>
      <Layout>
        <Routes>
          <Route path="/" element={<Overview />} />
          <Route path="/library" element={<Library />} />
          <Route path="/compression" element={<Compression />} />
          <Route path="/comparison" element={<Comparison />} />
          <Route path="/inference" element={<Inference />} />
          <Route path="/monitor" element={<Monitor />} />
          <Route path="/history" element={<History />} />
          <Route path="/settings" element={<SettingsPage />} />
          <Route
            path="*"
            element={
              <div className="panel p-8 text-center text-[13px] text-muted">
                This section does not exist. Use ⌘K to navigate.
              </div>
            }
          />
        </Routes>
      </Layout>
      <ToastHost />
    </>
  );
}
