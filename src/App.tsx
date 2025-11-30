import { useEffect, useState } from 'react'
import { ThunderstoreAPI } from './api/thunderstore'
import type { Community } from './types/thunderstore'
import { useProfileStore } from './store/useProfileStore'

function App() {
  const [communities, setCommunities] = useState<Community[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const { profiles, createProfile, loadProfiles, activeProfileId, selectProfile, deleteProfile } = useProfileStore()
  const [newProfileName, setNewProfileName] = useState('')

  useEffect(() => {
    loadProfiles()
    ThunderstoreAPI.getCommunities()
      .then(setCommunities)
      .catch(err => setError(err.message))
      .finally(() => setLoading(false))
  }, [])

  const handleCreateProfile = () => {
    if (!newProfileName) return
    createProfile(newProfileName, 'risk-of-rain-2') // Default game for test
    setNewProfileName('')
  }

  return (
    <div className="p-4 flex gap-4">
      <div className="w-1/3 border-r border-gray-700 pr-4">
        <h2 className="text-xl font-bold mb-4">Profiles</h2>
        <div className="flex gap-2 mb-4">
          <input
            className="bg-gray-800 p-2 rounded flex-1"
            placeholder="New Profile Name"
            value={newProfileName}
            onChange={e => setNewProfileName(e.target.value)}
          />
          <button onClick={handleCreateProfile} className="bg-blue-600 px-4 py-2 rounded">Add</button>
        </div>
        <div className="space-y-2">
          {profiles.map(p => (
            <div key={p.id} className={`p-3 rounded cursor-pointer flex justify-between items-center ${activeProfileId === p.id ? 'bg-blue-900' : 'bg-gray-800'}`} onClick={() => selectProfile(p.id)}>
              <div>
                <div className="font-bold">{p.name}</div>
                <div className="text-xs text-gray-400">{p.gameIdentifier}</div>
              </div>
              <button onClick={(e) => { e.stopPropagation(); deleteProfile(p.id) }} className="text-red-500 hover:text-red-300">X</button>
            </div>
          ))}
        </div>
      </div>

      <div className="w-2/3">
        <h1 className="text-2xl font-bold mb-4">Thunderstore Communities</h1>
        {loading && <p>Loading...</p>}
        {error && <p className="text-red-500">Error: {error}</p>}
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4 h-96 overflow-y-auto">
          {communities.map(c => (
            <div key={c.identifier} className="p-4 bg-gray-800 rounded-lg shadow">
              <h2 className="text-xl font-semibold">{c.name}</h2>
              <p className="text-gray-400">{c.identifier}</p>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

export default App
