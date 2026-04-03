Facter.add(:deep) do
  setcode do
    { 'a' =>
      {
        'b' => { 'd' => '1' },
        'c' => { 'e' => '2' }
      }
    }
  end
end
